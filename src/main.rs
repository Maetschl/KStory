mod content;
mod dioxus_ui;
mod project;

use clap::{Parser, Subcommand};
#[cfg(feature = "legacy-egui")]
use eframe::egui;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
#[cfg(feature = "legacy-egui")]
use std::time::{Duration, Instant};
use thiserror::Error;

const WAITING: &str = "waiting_for_translation";
const ON_REVIEW: &str = "on_review";
const ACCEPTED: &str = "accepted";
const CATALOG_SCHEMA_VERSION: u32 = 1;
#[cfg(feature = "legacy-egui")]
const EXPLORER_WIDTH: f32 = 320.0;
#[cfg(feature = "legacy-egui")]
const INSPECTOR_WIDTH: f32 = 320.0;
#[cfg(feature = "legacy-egui")]
const SIDE_PANEL_MIN_WIDTH: f32 = 260.0;
#[cfg(feature = "legacy-egui")]
const SIDE_PANEL_MAX_WIDTH: f32 = 520.0;

#[derive(Debug, Error)]
enum AppError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error in {path}: {source}")]
    Json {
        path: String,
        source: serde_json::Error,
    },
    #[error("validation failed: {0}")]
    Validation(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Catalog {
    schema_version: u32,
    source_locale: String,
    target_locales: Vec<String>,
    #[serde(default)]
    entries: BTreeMap<String, KeyEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
struct KeyEntry {
    #[serde(rename = "type")]
    entry_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    story_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    quest_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    dialogue_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    owner_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    speaker_id: Option<String>,
    #[serde(default)]
    variables: Vec<String>,
    #[serde(default)]
    tokens: Vec<String>,
    #[serde(default)]
    orphaned: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LocaleFile {
    #[serde(default = "default_catalog_schema_version")]
    schema_version: u32,
    locale: String,
    #[serde(default)]
    entries: BTreeMap<String, LocaleEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LocaleEntry {
    text: String,
    status: String,
    source_hash: String,
    text_hash: String,
    #[serde(default)]
    translated_by: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RuntimeFile {
    #[serde(default = "default_catalog_schema_version")]
    schema_version: u32,
    locale: String,
    entries: BTreeMap<String, String>,
}

fn default_catalog_schema_version() -> u32 {
    CATALOG_SCHEMA_VERSION
}

#[derive(Debug, Clone, Deserialize)]
struct DialogueCatalog {
    dialogues: BTreeMap<String, DialogueDefinition>,
}

#[derive(Debug, Clone, Deserialize)]
struct DialogueDefinition {
    #[serde(default)]
    story_id: Option<String>,
    #[serde(default)]
    owner_id: Option<String>,
    lines: Vec<DialogueLineDefinition>,
}

#[derive(Debug, Clone, Deserialize)]
struct DialogueLineDefinition {
    kind: String,
    #[serde(default)]
    key: Option<String>,
    #[serde(default)]
    speaker: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct QuestCatalog {
    quests: BTreeMap<String, QuestDefinition>,
}

#[derive(Debug, Clone, Deserialize)]
struct QuestDefinition {
    #[serde(default)]
    story_id: Option<String>,
    #[serde(default)]
    owner_id: Option<String>,
    #[serde(default)]
    description_keys: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct SyncReport {
    created_keys: usize,
    created_translations: usize,
    changed_sources: usize,
    changed_manual_translations: usize,
    orphaned_translations: usize,
    orphaned_keys: usize,
}

#[derive(Debug, Clone)]
struct Store {
    root: PathBuf,
    paths: project::ProjectPaths,
    catalog: Catalog,
    source: LocaleFile,
    targets: BTreeMap<String, LocaleFile>,
    content: content::ContentCatalog,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CreatedStory {
    story_id: String,
    entry_node_id: String,
    dialogue_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CreatedNode {
    story_id: String,
    node_id: String,
    dialogue_id: Option<String>,
    line_key: Option<String>,
    connection_id: Option<String>,
}

#[derive(Debug, Clone)]
struct ConnectionRequest {
    from_node_id: String,
    to_node_id: String,
    output_id: String,
    connection_kind: String,
    label_key: Option<String>,
    condition: Option<content::ConditionDefinition>,
}

impl Store {
    fn load(root: impl AsRef<Path>) -> Result<Self, AppError> {
        let paths = project::ProjectPaths::from_input(root).map_err(AppError::Validation)?;
        Self::load_paths(paths)
    }

    fn load_paths(paths: project::ProjectPaths) -> Result<Self, AppError> {
        let catalog: Catalog = read_json(&paths.keys)?;
        if catalog.schema_version != CATALOG_SCHEMA_VERSION {
            return Err(AppError::Validation(format!(
                "keys.json uses schema version {}; expected schema version {}",
                catalog.schema_version, CATALOG_SCHEMA_VERSION
            )));
        }
        let source: LocaleFile =
            read_json(&paths.locale_file(&catalog.source_locale)).or_else(|_| {
                let runtime: RuntimeFile = read_json(
                    &paths
                        .export
                        .join(&catalog.source_locale)
                        .with_extension("json"),
                )?;
                Ok::<LocaleFile, AppError>(LocaleFile {
                    schema_version: CATALOG_SCHEMA_VERSION,
                    locale: runtime.locale,
                    entries: runtime
                        .entries
                        .into_iter()
                        .map(|(key, text)| {
                            let hash = hash_text(&text);
                            (
                                key,
                                LocaleEntry {
                                    text,
                                    status: "source".into(),
                                    source_hash: hash.clone(),
                                    text_hash: hash,
                                    translated_by: None,
                                },
                            )
                        })
                        .collect(),
                })
            })?;
        if source.schema_version != CATALOG_SCHEMA_VERSION {
            return Err(AppError::Validation(format!(
                "locales/en.json uses schema version {}; expected schema version {}",
                source.schema_version, CATALOG_SCHEMA_VERSION
            )));
        }
        let mut targets = BTreeMap::new();
        for locale in &catalog.target_locales {
            let path = paths.locale_file(locale);
            let file = read_json(&path).unwrap_or_else(|_| LocaleFile {
                schema_version: CATALOG_SCHEMA_VERSION,
                locale: locale.clone(),
                entries: BTreeMap::new(),
            });
            if file.schema_version != CATALOG_SCHEMA_VERSION {
                return Err(AppError::Validation(format!(
                    "locales/{locale}.json uses schema version {}; expected schema version {}",
                    file.schema_version, CATALOG_SCHEMA_VERSION
                )));
            }
            targets.insert(locale.clone(), file);
        }
        let content = content::ContentCatalog::load_files(&content::ContentPaths {
            stories: paths.stories.clone(),
            dialogues: paths.dialogues.clone(),
            quests: paths.quests.clone(),
            items: paths.items.clone(),
            entities: paths.entities.clone(),
            callbacks: paths.callbacks.clone(),
        })
        .map_err(AppError::Validation)?;
        Ok(Self {
            root: paths.project_dir.clone(),
            paths,
            catalog,
            source,
            targets,
            content,
        })
    }

    fn save(&self) -> Result<(), AppError> {
        write_json(&self.paths.keys, &self.catalog)?;
        write_json(
            &self.paths.locale_file(&self.catalog.source_locale),
            &self.source,
        )?;
        for (locale, file) in &self.targets {
            write_json(&self.paths.locale_file(locale), file)?;
        }
        Ok(())
    }

    fn save_content(&self) -> Result<(), AppError> {
        self.content
            .save_files(&content::ContentPaths {
                stories: self.paths.stories.clone(),
                dialogues: self.paths.dialogues.clone(),
                quests: self.paths.quests.clone(),
                items: self.paths.items.clone(),
                entities: self.paths.entities.clone(),
                callbacks: self.paths.callbacks.clone(),
            })
            .map_err(AppError::Validation)
    }

    fn sync(&mut self) -> Result<SyncReport, AppError> {
        self.import_runtime_source_if_empty()?;
        self.import_runtime_targets_if_empty()?;
        let mut report = SyncReport::default();
        let source_keys: BTreeSet<String> = self.source.entries.keys().cloned().collect();
        let content_metadata = self.content_metadata()?;

        for key in &source_keys {
            if let Some(metadata) = content_metadata.get(key) {
                if let Some(existing) = self.catalog.entries.get_mut(key) {
                    existing.entry_type = metadata.entry_type.clone();
                    existing.story_id = metadata.story_id.clone().or(existing.story_id.clone());
                    existing.quest_id = metadata.quest_id.clone().or(existing.quest_id.clone());
                    existing.dialogue_id = metadata
                        .dialogue_id
                        .clone()
                        .or(existing.dialogue_id.clone());
                    existing.owner_id = metadata.owner_id.clone().or(existing.owner_id.clone());
                    existing.speaker_id =
                        metadata.speaker_id.clone().or(existing.speaker_id.clone());
                }
            }
            if !self.catalog.entries.contains_key(key) {
                let metadata = content_metadata
                    .get(key)
                    .cloned()
                    .unwrap_or_else(|| KeyEntry {
                        entry_type: infer_type(key),
                        ..Default::default()
                    });
                self.catalog.entries.insert(key.clone(), metadata);
                report.created_keys += 1;
            }
            let source_hash = hash_text(&self.source.entries[key].text);
            for file in self.targets.values_mut() {
                match file.entries.get_mut(key) {
                    None => {
                        file.entries.insert(
                            key.clone(),
                            LocaleEntry {
                                text: String::new(),
                                status: WAITING.into(),
                                source_hash: source_hash.clone(),
                                text_hash: hash_text(""),
                                translated_by: None,
                            },
                        );
                        report.created_translations += 1;
                    }
                    Some(entry) => {
                        if entry.source_hash != source_hash {
                            entry.status = WAITING.into();
                            entry.source_hash = source_hash.clone();
                            report.changed_sources += 1;
                        }
                        let actual_text_hash = hash_text(&entry.text);
                        if actual_text_hash != entry.text_hash && entry.status != WAITING {
                            entry.status = WAITING.into();
                            report.changed_manual_translations += 1;
                        }
                        entry.text_hash = actual_text_hash;
                    }
                }
            }
        }

        for (key, metadata) in &mut self.catalog.entries {
            let orphaned = !source_keys.contains(key);
            if orphaned && !metadata.orphaned {
                report.orphaned_keys += 1;
            }
            metadata.orphaned = orphaned;
        }

        for file in self.targets.values() {
            report.orphaned_translations += file
                .entries
                .keys()
                .filter(|key| !source_keys.contains(*key))
                .count();
        }
        self.save()?;
        Ok(report)
    }

    fn import_runtime_source_if_empty(&mut self) -> Result<(), AppError> {
        let runtime_path = self
            .paths
            .export
            .join(format!("{}.json", self.catalog.source_locale));
        if !runtime_path.exists() {
            return Ok(());
        }
        let runtime: RuntimeFile = read_json(&runtime_path)?;
        for (key, text) in runtime.entries {
            if self.source.entries.contains_key(&key) {
                continue;
            }
            let hash = hash_text(&text);
            self.source.entries.insert(
                key,
                LocaleEntry {
                    text,
                    status: "source".into(),
                    source_hash: hash.clone(),
                    text_hash: hash,
                    translated_by: None,
                },
            );
        }
        self.save_content()?;
        Ok(())
    }

    fn import_runtime_targets_if_empty(&mut self) -> Result<(), AppError> {
        for (locale, file) in self.targets.iter_mut() {
            if !file.entries.is_empty() {
                continue;
            }
            let runtime_path = self.paths.export.join(format!("{locale}.json"));
            if !runtime_path.exists() {
                continue;
            }
            let runtime: RuntimeFile = read_json(&runtime_path)?;
            for (key, text) in runtime.entries {
                let source_hash = self
                    .source
                    .entries
                    .get(&key)
                    .map(|entry| hash_text(&entry.text))
                    .unwrap_or_default();
                let text_hash = hash_text(&text);
                file.entries.insert(
                    key,
                    LocaleEntry {
                        text,
                        status: ACCEPTED.into(),
                        source_hash,
                        text_hash,
                        translated_by: Some("bootstrap".into()),
                    },
                );
            }
        }
        Ok(())
    }

    fn content_metadata(&self) -> Result<BTreeMap<String, KeyEntry>, AppError> {
        let path = self.paths.dialogues.clone();
        let mut result = BTreeMap::new();
        if path.exists() {
            let catalog: DialogueCatalog = read_json(&path)?;
            for (dialogue_id, dialogue) in catalog.dialogues {
                for line in dialogue.lines {
                    if line.kind != "text" {
                        continue;
                    }
                    if let Some(key) = line.key {
                        result.insert(
                            key,
                            KeyEntry {
                                entry_type: "dialogue_line".into(),
                                story_id: dialogue.story_id.clone(),
                                quest_id: None,
                                dialogue_id: Some(dialogue_id.clone()),
                                owner_id: dialogue.owner_id.clone(),
                                speaker_id: line.speaker,
                                variables: Vec::new(),
                                tokens: Vec::new(),
                                orphaned: false,
                            },
                        );
                    }
                }
            }
        }

        let quests_path = self.paths.quests.clone();
        if quests_path.exists() {
            let quests: QuestCatalog = read_json(&quests_path)?;
            for (quest_id, quest) in quests.quests {
                for key in quest.description_keys {
                    result.insert(
                        key,
                        KeyEntry {
                            entry_type: "quest_description".into(),
                            story_id: quest.story_id.clone(),
                            quest_id: Some(quest_id.clone()),
                            dialogue_id: None,
                            owner_id: quest.owner_id.clone(),
                            speaker_id: None,
                            variables: Vec::new(),
                            tokens: Vec::new(),
                            orphaned: false,
                        },
                    );
                }
            }
        }
        Ok(result)
    }

    fn validate(&self) -> Result<Vec<String>, AppError> {
        let mut errors = Vec::new();
        for (key, source) in &self.source.entries {
            if !self.catalog.entries.contains_key(key) {
                errors.push(format!("missing key metadata: {key}"));
            }
            for (locale, file) in &self.targets {
                if let Some(entry) = file.entries.get(key) {
                    if ![WAITING, ON_REVIEW, ACCEPTED].contains(&entry.status.as_str()) {
                        errors.push(format!(
                            "{locale}:{key} has invalid status {}",
                            entry.status
                        ));
                    }
                    if entry.status == ACCEPTED && entry.source_hash != hash_text(&source.text) {
                        errors.push(format!("{locale}:{key} is accepted against a stale source"));
                    }
                    if !entry.text.is_empty() {
                        let source_placeholders = placeholders(&source.text);
                        let target_placeholders = placeholders(&entry.text);
                        if source_placeholders != target_placeholders {
                            errors.push(format!("{locale}:{key} has incompatible placeholders"));
                        }
                        if source.text.matches('\n').count() != entry.text.matches('\n').count() {
                            errors.push(format!("{locale}:{key} has incompatible line breaks"));
                        }
                        if self.expected_commands(key, &source.text)
                            != commands_in_text(&entry.text)
                        {
                            errors.push(format!(
                                "{locale}:{key} has incompatible commands or tokens"
                            ));
                        }
                    }
                }
            }
        }
        let source_keys: BTreeSet<String> = self.source.entries.keys().cloned().collect();
        errors.extend(self.content.validate(&source_keys));
        if errors.is_empty() {
            Ok(errors)
        } else {
            Err(AppError::Validation(errors.join("\n")))
        }
    }

    fn list(&self, locale: &str, status: &str) -> Vec<(String, String)> {
        self.targets
            .get(locale)
            .map(|file| {
                file.entries
                    .iter()
                    .filter(|(_, entry)| entry.status == status)
                    .map(|(key, entry)| (key.clone(), entry.text.clone()))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn validation_errors_for_text(&self, key: &str, text: &str) -> Vec<String> {
        let Some(source) = self.source.entries.get(key) else {
            return vec![format!("unknown source key {key}")];
        };
        let mut errors = Vec::new();
        if text.trim().is_empty() {
            errors.push("translation text is empty".into());
        }
        if placeholders(&source.text) != placeholders(text) {
            errors.push("variables/placeholders do not match the English source".into());
        }
        if source.text.matches('\n').count() != text.matches('\n').count() {
            errors.push("line break count does not match the English source".into());
        }
        if self.expected_commands(key, &source.text) != commands_in_text(text) {
            errors.push("commands/tokens do not match the English source".into());
        }
        errors
    }

    fn expected_commands(&self, key: &str, source_text: &str) -> BTreeSet<String> {
        let mut commands = commands_in_text(source_text);
        if let Some(metadata) = self.catalog.entries.get(key) {
            commands.extend(metadata.tokens.iter().cloned());
        }
        commands
    }

    fn validate_entry(&self, locale: &str, key: &str) -> Vec<String> {
        let Some(source) = self.source.entries.get(key) else {
            return vec![format!("unknown source key {key}")];
        };
        if locale == self.catalog.source_locale {
            return Vec::new();
        }
        let Some(file) = self.targets.get(locale) else {
            return vec![format!("unknown locale {locale}")];
        };
        let Some(entry) = file.entries.get(key) else {
            return vec!["translation entry is missing".into()];
        };
        let mut errors = self.validation_errors_for_text(key, &entry.text);
        if entry.status == ACCEPTED && entry.source_hash != hash_text(&source.text) {
            errors.push("accepted translation has an obsolete source hash".into());
        }
        if ![WAITING, ON_REVIEW, ACCEPTED].contains(&entry.status.as_str()) {
            errors.push(format!("unknown translation status {}", entry.status));
        }
        errors
    }

    fn entry_view(&self, locale: &str, key: &str) -> Option<EntryViewModel> {
        let metadata = self.catalog.entries.get(key)?.clone();
        let source = self.source.entries.get(key);
        let target = self
            .targets
            .get(locale)
            .and_then(|file| file.entries.get(key));
        let source_locale = locale == self.catalog.source_locale;
        let source_text = source.map(|entry| entry.text.clone()).unwrap_or_default();
        let source_hash = source
            .map(|entry| hash_text(&entry.text))
            .unwrap_or_default();
        Some(EntryViewModel {
            key: key.into(),
            source_text: source_text.clone(),
            target_text: if source_locale {
                source_text.clone()
            } else {
                target.map(|entry| entry.text.clone()).unwrap_or_default()
            },
            status: if source_locale {
                "source".into()
            } else {
                target
                    .map(|entry| entry.status.clone())
                    .unwrap_or_else(|| WAITING.into())
            },
            metadata,
            source_hash: source_hash.clone(),
            stored_source_hash: if source_locale {
                source_hash.clone()
            } else {
                target
                    .map(|entry| entry.source_hash.clone())
                    .unwrap_or_default()
            },
            text_hash: if source_locale {
                source_hash
            } else {
                target
                    .map(|entry| entry.text_hash.clone())
                    .unwrap_or_default()
            },
            translated_by: if source_locale {
                None
            } else {
                target.and_then(|entry| entry.translated_by.clone())
            },
            validation_errors: self.validate_entry(locale, key),
        })
    }

    fn localized_preview(&self, locale: &str, key: &str) -> (String, bool, String) {
        let source = self.source.entries.get(key);
        let source_text = source
            .map(|entry| entry.text.clone())
            .unwrap_or_else(|| format!("Missing key: {key}"));
        if locale == self.catalog.source_locale {
            return (source_text, false, "source".into());
        }
        let current_source_hash = source.map(|entry| hash_text(&entry.text));
        if let Some(entry) = self
            .targets
            .get(locale)
            .and_then(|file| file.entries.get(key))
        {
            if self.effective_translation_status(locale, key) == ACCEPTED
                && Some(entry.source_hash.clone()) == current_source_hash
            {
                return (entry.text.clone(), false, ACCEPTED.into());
            }
            return (
                source_text,
                true,
                self.effective_translation_status(locale, key).into(),
            );
        }
        (source_text, true, "missing".into())
    }

    fn effective_translation_status(&self, locale: &str, key: &str) -> &'static str {
        if locale == self.catalog.source_locale {
            return "source";
        }
        let Some(source) = self.source.entries.get(key) else {
            return WAITING;
        };
        let Some(entry) = self
            .targets
            .get(locale)
            .and_then(|file| file.entries.get(key))
        else {
            return WAITING;
        };
        if entry.text.trim().is_empty() || entry.status == WAITING {
            return WAITING;
        }
        if entry.status == ACCEPTED {
            let source_hash = hash_text(&source.text);
            if entry.source_hash == source_hash
                && self.validation_errors_for_text(key, &entry.text).is_empty()
            {
                return ACCEPTED;
            }
            return WAITING;
        }
        if entry.status == ON_REVIEW {
            return ON_REVIEW;
        }
        WAITING
    }

    fn status_counts(&self, locale: &str) -> BTreeMap<String, usize> {
        let mut counts = BTreeMap::new();
        for key in self.catalog.entries.keys() {
            let status = self.effective_translation_status(locale, key);
            *counts.entry(status.to_string()).or_default() += 1;
        }
        counts
    }

    fn filtered_keys(&self, locale: &str, filters: &FilterState) -> Vec<String> {
        let query = filters.search.trim().to_lowercase();
        let mut keys: Vec<String> = self
            .catalog
            .entries
            .iter()
            .filter(|(key, metadata)| {
                let source = self
                    .source
                    .entries
                    .get(*key)
                    .map(|entry| entry.text.as_str())
                    .unwrap_or("");
                let target = self
                    .targets
                    .get(locale)
                    .and_then(|file| file.entries.get(*key));
                let translation = target.map(|entry| entry.text.as_str()).unwrap_or("");
                let status = self.effective_translation_status(locale, key);
                let searchable = [
                    key.as_str(),
                    source,
                    translation,
                    metadata.story_id.as_deref().unwrap_or(""),
                    metadata.quest_id.as_deref().unwrap_or(""),
                    metadata.owner_id.as_deref().unwrap_or(""),
                    metadata.speaker_id.as_deref().unwrap_or(""),
                    metadata.entry_type.as_str(),
                ]
                .join(" ")
                .to_lowercase();
                let query_match = query.is_empty() || searchable.contains(&query);
                let status_match = filters.status.is_empty() || filters.status == status;
                let type_match =
                    filters.entry_type.is_empty() || filters.entry_type == metadata.entry_type;
                let story_match = filters.story.is_empty()
                    || metadata.story_id.as_deref() == Some(filters.story.as_str());
                let quest_match = filters.quest.is_empty()
                    || metadata.quest_id.as_deref() == Some(filters.quest.as_str());
                let owner_match = filters.owner.is_empty()
                    || metadata.owner_id.as_deref() == Some(filters.owner.as_str());
                let orphan_match = !filters.only_orphaned || metadata.orphaned;
                query_match
                    && status_match
                    && type_match
                    && story_match
                    && quest_match
                    && owner_match
                    && orphan_match
            })
            .map(|(key, _)| key.clone())
            .collect();

        keys.sort_by(|left, right| {
            let left_metadata = &self.catalog.entries[left];
            let right_metadata = &self.catalog.entries[right];
            let left_status = self.effective_translation_status(locale, left);
            let right_status = self.effective_translation_status(locale, right);
            let ordering = match filters.sort_mode {
                SortMode::Key => left.cmp(right),
                SortMode::Status => left_status.cmp(right_status).then_with(|| left.cmp(right)),
                SortMode::Story => left_metadata
                    .story_id
                    .as_deref()
                    .unwrap_or("")
                    .cmp(right_metadata.story_id.as_deref().unwrap_or(""))
                    .then_with(|| left.cmp(right)),
                SortMode::Owner => left_metadata
                    .owner_id
                    .as_deref()
                    .unwrap_or("")
                    .cmp(right_metadata.owner_id.as_deref().unwrap_or(""))
                    .then_with(|| left.cmp(right)),
            };
            ordering
        });
        keys
    }

    fn save_translation(&mut self, locale: &str, key: &str, text: String) -> Result<(), AppError> {
        let errors = self.validation_errors_for_text(key, &text);
        if !errors.is_empty() {
            return Err(AppError::Validation(errors.join("; ")));
        }
        let source_hash = self
            .source
            .entries
            .get(key)
            .map(|entry| hash_text(&entry.text))
            .ok_or_else(|| AppError::Validation(format!("unknown source key {key}")))?;
        let file = self
            .targets
            .get_mut(locale)
            .ok_or_else(|| AppError::Validation(format!("unknown locale {locale}")))?;
        let entry = file
            .entries
            .get_mut(key)
            .ok_or_else(|| AppError::Validation(format!("unknown key {key}")))?;
        entry.text = text;
        entry.status = WAITING.into();
        entry.source_hash = source_hash;
        entry.text_hash = hash_text(&entry.text);
        entry.translated_by = Some("manual".into());
        self.save()
    }

    fn mark_on_review(&mut self, locale: &str, key: &str) -> Result<(), AppError> {
        let text = self
            .targets
            .get(locale)
            .and_then(|file| file.entries.get(key))
            .map(|entry| entry.text.clone())
            .ok_or_else(|| AppError::Validation(format!("unknown key {key}")))?;
        let errors = self.validation_errors_for_text(key, &text);
        if !errors.is_empty() {
            return Err(AppError::Validation(errors.join("; ")));
        }
        let file = self
            .targets
            .get_mut(locale)
            .ok_or_else(|| AppError::Validation(format!("unknown locale {locale}")))?;
        let entry = file
            .entries
            .get_mut(key)
            .ok_or_else(|| AppError::Validation(format!("unknown key {key}")))?;
        entry.status = ON_REVIEW.into();
        entry.text_hash = hash_text(&entry.text);
        self.save()
    }

    fn reset_translation(&mut self, locale: &str, key: &str) -> Result<(), AppError> {
        let source_hash = self
            .source
            .entries
            .get(key)
            .map(|entry| hash_text(&entry.text))
            .ok_or_else(|| AppError::Validation(format!("unknown source key {key}")))?;
        let file = self
            .targets
            .get_mut(locale)
            .ok_or_else(|| AppError::Validation(format!("unknown locale {locale}")))?;
        let entry = file
            .entries
            .get_mut(key)
            .ok_or_else(|| AppError::Validation(format!("unknown key {key}")))?;
        entry.text.clear();
        entry.status = WAITING.into();
        entry.source_hash = source_hash;
        entry.text_hash = hash_text("");
        entry.translated_by = None;
        self.save()
    }

    fn accept(&mut self, locale: &str, key: &str) -> Result<(), AppError> {
        let source_hash = self
            .source
            .entries
            .get(key)
            .ok_or_else(|| AppError::Validation(format!("unknown source key {key}")))?
            .text
            .clone();
        let source_hash = hash_text(&source_hash);
        let target_text = self
            .targets
            .get(locale)
            .and_then(|file| file.entries.get(key))
            .ok_or_else(|| AppError::Validation(format!("unknown key {key}")))?
            .text
            .clone();
        let validation_errors = self.validation_errors_for_text(key, &target_text);
        if !validation_errors.is_empty() {
            return Err(AppError::Validation(validation_errors.join("; ")));
        }
        let file = self
            .targets
            .get_mut(locale)
            .ok_or_else(|| AppError::Validation(format!("unknown locale {locale}")))?;
        let entry = file
            .entries
            .get_mut(key)
            .ok_or_else(|| AppError::Validation(format!("unknown key {key}")))?;
        if entry.status != ON_REVIEW {
            return Err(AppError::Validation(format!(
                "{locale}:{key} is not on review"
            )));
        }
        if entry.source_hash != source_hash {
            return Err(AppError::Validation(format!(
                "{locale}:{key} has stale source"
            )));
        }
        if entry.text.trim().is_empty() {
            return Err(AppError::Validation(format!(
                "{locale}:{key} has no translation text"
            )));
        }
        entry.status = ACCEPTED.into();
        self.save()
    }

    fn accept_all_on_review(&mut self, locale: &str) -> Result<usize, AppError> {
        let keys: Vec<String> = self
            .targets
            .get(locale)
            .map(|file| {
                file.entries
                    .iter()
                    .filter(|(_, entry)| entry.status == ON_REVIEW)
                    .map(|(key, _)| key.clone())
                    .collect()
            })
            .unwrap_or_default();
        let mut accepted = 0;
        for key in keys {
            if self.accept(locale, &key).is_ok() {
                accepted += 1;
            }
        }
        Ok(accepted)
    }

    fn reject(&mut self, locale: &str, key: &str) -> Result<(), AppError> {
        let file = self
            .targets
            .get_mut(locale)
            .ok_or_else(|| AppError::Validation(format!("unknown locale {locale}")))?;
        let entry = file
            .entries
            .get_mut(key)
            .ok_or_else(|| AppError::Validation(format!("unknown key {key}")))?;
        entry.status = WAITING.into();
        self.save()
    }

    fn set_translation(
        &mut self,
        locale: &str,
        key: &str,
        text: String,
        translated_by: String,
    ) -> Result<(), AppError> {
        let source = self
            .source
            .entries
            .get(key)
            .ok_or_else(|| AppError::Validation(format!("unknown source key {key}")))?;
        let source_hash = hash_text(&source.text);
        let validation_errors = self.validation_errors_for_text(key, &text);
        if !validation_errors.is_empty() {
            return Err(AppError::Validation(format!(
                "{locale}:{key} {}",
                validation_errors.join("; ")
            )));
        }
        let file = self
            .targets
            .get_mut(locale)
            .ok_or_else(|| AppError::Validation(format!("unknown locale {locale}")))?;
        let entry = file.entries.entry(key.to_string()).or_insert(LocaleEntry {
            text: String::new(),
            status: WAITING.into(),
            source_hash: source_hash.clone(),
            text_hash: hash_text(""),
            translated_by: None,
        });
        entry.text = text;
        entry.status = ON_REVIEW.into();
        entry.source_hash = source_hash;
        entry.text_hash = hash_text(&entry.text);
        entry.translated_by = Some(translated_by);
        self.save()
    }

    fn add_key(
        &mut self,
        key: &str,
        text: String,
        entry_type: String,
        story_id: Option<String>,
        owner_id: Option<String>,
    ) -> Result<(), AppError> {
        self.add_key_in_memory(key, text, entry_type, story_id, owner_id)?;
        self.save()
    }

    fn add_key_in_memory(
        &mut self,
        key: &str,
        text: String,
        entry_type: String,
        story_id: Option<String>,
        owner_id: Option<String>,
    ) -> Result<(), AppError> {
        if self.source.entries.contains_key(key) {
            return Err(AppError::Validation(format!("key already exists: {key}")));
        }
        let source_hash = hash_text(&text);
        self.source.entries.insert(
            key.to_string(),
            LocaleEntry {
                text,
                status: "source".into(),
                source_hash: source_hash.clone(),
                text_hash: source_hash.clone(),
                translated_by: None,
            },
        );
        self.catalog.entries.insert(
            key.to_string(),
            KeyEntry {
                entry_type,
                story_id,
                owner_id,
                ..Default::default()
            },
        );
        for file in self.targets.values_mut() {
            file.entries.insert(
                key.to_string(),
                LocaleEntry {
                    text: String::new(),
                    status: WAITING.into(),
                    source_hash: source_hash.clone(),
                    text_hash: hash_text(""),
                    translated_by: None,
                },
            );
        }
        Ok(())
    }

    fn create_story(&mut self) -> Result<CreatedStory, AppError> {
        let story_id = next_content_id("story", self.content.stories.keys());
        let dialogue_id = format!("dialogue.{story_id}.intro");
        let entry_node_id = format!("{story_id}.start");
        let line_id = format!("{dialogue_id}.line.start");
        let name_key = format!("story.{story_id}.name");
        let description_key = format!("story.{story_id}.description");
        let chapter_key = format!("story.{story_id}.chapter.main");
        let node_key = format!("story.{story_id}.node.start");
        let line_key = format!("dialog.{story_id}.intro.start");
        let snapshot = (
            self.catalog.clone(),
            self.source.clone(),
            self.targets.clone(),
            self.content.clone(),
        );

        let result = (|| {
            self.add_key_in_memory(
                &name_key,
                "New story".into(),
                "story_name".into(),
                Some(story_id.clone()),
                Some("content.author".into()),
            )?;
            self.add_key_in_memory(
                &description_key,
                "A new story.".into(),
                "story_description".into(),
                Some(story_id.clone()),
                Some("content.author".into()),
            )?;
            self.add_key_in_memory(
                &chapter_key,
                "Main".into(),
                "chapter_name".into(),
                Some(story_id.clone()),
                Some("content.author".into()),
            )?;
            self.add_key_in_memory(
                &node_key,
                "Start".into(),
                "story_node_name".into(),
                Some(story_id.clone()),
                Some("content.author".into()),
            )?;
            self.add_key_in_memory(
                &line_key,
                "This is the beginning of a new story.".into(),
                "dialogue_line".into(),
                Some(story_id.clone()),
                Some("content.author".into()),
            )?;
            self.content.dialogues.insert(
                dialogue_id.clone(),
                content::DialogueDefinition {
                    name_key: name_key.clone(),
                    story_id: Some(story_id.clone()),
                    chapter_id: Some("main".into()),
                    owner_id: Some("content.author".into()),
                    lines: vec![content::DialogueLineDefinition {
                        id: line_id.clone(),
                        kind: "text".into(),
                        key: Some(line_key),
                        command: None,
                        speaker: Some("Narrator".into()),
                        variables: Vec::new(),
                        on_enter: None,
                        on_exit: None,
                    }],
                    on_enter: Vec::new(),
                    on_exit: Vec::new(),
                },
            );
            self.content.stories.insert(
                story_id.clone(),
                content::StoryDefinition {
                    name_key,
                    description_key,
                    owner_id: "content.author".into(),
                    entry_node_id: entry_node_id.clone(),
                    chapters: vec![content::ChapterDefinition {
                        id: "main".into(),
                        name_key: chapter_key,
                        node_ids: vec![entry_node_id.clone()],
                    }],
                    nodes: BTreeMap::from([(
                        entry_node_id.clone(),
                        content::StoryNode {
                            node_type: "dialogue".into(),
                            name_key: node_key,
                            payload: Some(content::NodePayload::Dialogue {
                                dialogue_id: dialogue_id.clone(),
                                line_id: line_id.clone(),
                            }),
                            chapter_id: Some("main".into()),
                            dialogue_id: Some(dialogue_id.clone()),
                            dialogue_line_id: Some(line_id.clone()),
                            quest_id: None,
                            scene_id: None,
                            event_id: None,
                            action: None,
                            condition: None,
                            choices: Vec::new(),
                            next: Vec::new(),
                            position: [420.0, 48.0],
                        },
                    )]),
                    connections: BTreeMap::new(),
                },
            );
            self.save()?;
            Ok(CreatedStory {
                story_id: story_id.clone(),
                entry_node_id: entry_node_id.clone(),
                dialogue_id: dialogue_id.clone(),
            })
        })();

        if result.is_err() {
            self.catalog = snapshot.0;
            self.source = snapshot.1;
            self.targets = snapshot.2;
            self.content = snapshot.3;
        }
        result
    }

    /// Split the seven blacksmith QuestDialog scenes into independently executable
    /// authoring stories. The dialogue IDs and line IDs remain unchanged so the
    /// legacy Java flow keeps resolving the same text.
    fn migrate_quest_dialog_scenes(&mut self) -> Result<Vec<String>, AppError> {
        const SCENES: &[(&str, &str, &str)] = &[
            (
                "npc_blacksmith",
                "Blacksmith introduction",
                "npc_blacksmith",
            ),
            ("nice_lamp", "The lamp", "nice_lamp"),
            (
                "save_blacksmith_01",
                "Rescue the blacksmith",
                "save_blacksmith_01",
            ),
            (
                "blacksmith_city_help",
                "Help in the city",
                "blacksmith_city_help",
            ),
            (
                "blacksmith_missing_hammer",
                "The wrong hammer",
                "blacksmith_missing_hammer",
            ),
            (
                "blacksmith_found_hammer",
                "The blacksmith's hammer",
                "blacksmith_found_hammer",
            ),
            ("blacksmith_idle", "Blacksmith idle", "blacksmith_idle"),
        ];

        let snapshot = (
            self.catalog.clone(),
            self.source.clone(),
            self.targets.clone(),
            self.content.clone(),
        );
        let result = (|| {
            let mut created = Vec::new();
            for (dialogue_id, display_name, story_suffix) in SCENES {
                let story_id = format!("blacksmith.{story_suffix}");
                if self.content.stories.contains_key(&story_id) {
                    continue;
                }
                let dialogue = self
                    .content
                    .dialogues
                    .get(*dialogue_id)
                    .cloned()
                    .ok_or_else(|| {
                        AppError::Validation(format!(
                            "cannot migrate QuestDialog scene {dialogue_id}: dialogue is missing"
                        ))
                    })?;
                let owner_id = dialogue
                    .owner_id
                    .clone()
                    .unwrap_or_else(|| "npc.blacksmith".into());
                let name_key = format!("ui.story.{story_id}.name");
                let description_key = format!("ui.story.{story_id}.description");
                let chapter_key = format!("ui.story.{story_id}.chapter.main");
                self.add_key_in_memory(
                    &name_key,
                    (*display_name).into(),
                    "story_name".into(),
                    Some(story_id.clone()),
                    Some(owner_id.clone()),
                )?;
                self.add_key_in_memory(
                    &description_key,
                    format!("QuestDialog scene {dialogue_id}."),
                    "story_description".into(),
                    Some(story_id.clone()),
                    Some(owner_id.clone()),
                )?;
                self.add_key_in_memory(
                    &chapter_key,
                    "Main".into(),
                    "chapter_name".into(),
                    Some(story_id.clone()),
                    Some(owner_id.clone()),
                )?;

                let mut nodes = BTreeMap::new();
                let mut node_ids = Vec::new();
                for (index, line) in dialogue.lines.iter().enumerate() {
                    let node_id = format!("{story_id}.line_{index}");
                    let command = line.command.as_deref().unwrap_or_default();
                    let (node_type, payload, dialogue_ref, line_ref, action) =
                        if line.kind == "command" {
                            let (node_type, payload) =
                                command_node_definition(command, &self.content.items);
                            let action = match &payload {
                                content::NodePayload::Action(action) => Some(action.clone()),
                                _ => None,
                            };
                            (node_type, Some(payload), None, None, action)
                        } else {
                            (
                                "dialogue".into(),
                                Some(content::NodePayload::Dialogue {
                                    dialogue_id: (*dialogue_id).into(),
                                    line_id: line.id.clone(),
                                }),
                                Some((*dialogue_id).into()),
                                Some(line.id.clone()),
                                None,
                            )
                        };
                    let node_name_key = if line.kind == "text" {
                        line.key
                            .clone()
                            .unwrap_or_else(|| format!("{story_id}.line_{index}"))
                    } else {
                        let key = format!("ui.story.{story_id}.line_{index}.name");
                        self.add_key_in_memory(
                            &key,
                            command.to_string(),
                            "story_node_name".into(),
                            Some(story_id.clone()),
                            Some(owner_id.clone()),
                        )?;
                        key
                    };
                    nodes.insert(
                        node_id.clone(),
                        content::StoryNode {
                            node_type,
                            name_key: node_name_key,
                            payload,
                            chapter_id: Some("main".into()),
                            dialogue_id: dialogue_ref,
                            dialogue_line_id: line_ref,
                            quest_id: None,
                            scene_id: None,
                            event_id: None,
                            action,
                            condition: None,
                            choices: Vec::new(),
                            next: Vec::new(),
                            position: [420.0, 80.0 + index as f32 * 164.0],
                        },
                    );
                    node_ids.push(node_id);
                }

                let mut connections = BTreeMap::new();
                for index in 0..node_ids.len().saturating_sub(1) {
                    let id = format!("edge.{story_id}.line_{index}");
                    connections.insert(
                        id.clone(),
                        content::ConnectionDefinition {
                            id,
                            from_node_id: node_ids[index].clone(),
                            to_node_id: node_ids[index + 1].clone(),
                            output_id: "default".into(),
                            connection_kind: "next".into(),
                            label_key: None,
                            condition: None,
                        },
                    );
                }
                self.content.stories.insert(
                    story_id.clone(),
                    content::StoryDefinition {
                        name_key,
                        description_key,
                        owner_id: owner_id.clone(),
                        entry_node_id: node_ids.first().cloned().unwrap_or_default(),
                        chapters: vec![content::ChapterDefinition {
                            id: "main".into(),
                            name_key: chapter_key,
                            node_ids,
                        }],
                        nodes,
                        connections,
                    },
                );
                if let Some(dialogue) = self.content.dialogues.get_mut(*dialogue_id) {
                    dialogue.story_id = Some(story_id.clone());
                    dialogue.chapter_id = Some("main".into());
                }
                for metadata in self.catalog.entries.values_mut() {
                    if metadata.dialogue_id.as_deref() == Some(*dialogue_id) {
                        metadata.story_id = Some(story_id.clone());
                    }
                }
                created.push(story_id);
            }
            let blacksmith_story_ids = [
                "blacksmith.npc_blacksmith",
                "blacksmith.nice_lamp",
                "blacksmith.save_blacksmith_01",
                "blacksmith.blacksmith_city_help",
                "blacksmith.blacksmith_missing_hammer",
                "blacksmith.blacksmith_found_hammer",
                "blacksmith.blacksmith_idle",
            ];
            let blacksmith_is_dialogue_only = self
                .content
                .stories
                .get("blacksmith")
                .map(|story| {
                    story.nodes.values().all(|node| {
                        node.dialogue_id.as_deref().is_some_and(|dialogue_id| {
                            SCENES
                                .iter()
                                .any(|(scene_id, _, _)| *scene_id == dialogue_id)
                        })
                    })
                })
                .unwrap_or(false);
            if blacksmith_is_dialogue_only
                && blacksmith_story_ids
                    .iter()
                    .all(|story_id| self.content.stories.contains_key(*story_id))
            {
                self.content.stories.remove("blacksmith");
            }
            self.content
                .save(&self.root)
                .map_err(AppError::Validation)?;
            self.save()?;
            Ok(created)
        })();
        if result.is_err() {
            self.catalog = snapshot.0;
            self.source = snapshot.1;
            self.targets = snapshot.2;
            self.content = snapshot.3;
        }
        result
    }

    /// Split the six opening/gameplay QuestDialog scenes into independent stories.
    ///
    /// These dialogs are not one continuous story in the game: each one is
    /// started by a different scene or interaction. The dialogue and line IDs
    /// remain unchanged so the legacy Java flow keeps resolving the same text,
    /// while the authoring model gets one executable story per QuestDialog.
    fn migrate_opening_quest_dialog_scenes(&mut self) -> Result<Vec<String>, AppError> {
        const SCENES: &[(&str, &str, &str)] = &[
            ("init_text_0", "Opening: the earthquake", "story.intro"),
            ("init_text_1", "Opening: the lamp", "story.intro"),
            ("init_pre_board", "Before the quest board", "story.intro"),
            ("init_board", "Quest board", "story.workbench"),
            (
                "after_workbench_repair",
                "After workbench repair",
                "story.workbench",
            ),
            ("well_repair_quest", "Well repair", "story.well"),
        ];

        let snapshot = (
            self.catalog.clone(),
            self.source.clone(),
            self.targets.clone(),
            self.content.clone(),
        );
        let result = (|| {
            let mut created = Vec::new();

            for (dialogue_id, display_name, owner_id) in SCENES {
                let story_id = format!("quest_dialog.{dialogue_id}");

                if self.content.stories.contains_key(&story_id) {
                    if let Some(dialogue) = self.content.dialogues.get_mut(*dialogue_id) {
                        dialogue.story_id = Some(story_id.clone());
                        dialogue.chapter_id = Some("main".into());
                    }
                    continue;
                }

                let dialogue = self
                    .content
                    .dialogues
                    .get(*dialogue_id)
                    .cloned()
                    .ok_or_else(|| {
                        AppError::Validation(format!(
                            "cannot migrate QuestDialog scene {dialogue_id}: dialogue is missing"
                        ))
                    })?;
                let name_key = format!("ui.story.{story_id}.name");
                let description_key = format!("ui.story.{story_id}.description");
                let chapter_key = format!("ui.story.{story_id}.chapter.main");

                self.add_key_in_memory(
                    &name_key,
                    (*display_name).into(),
                    "story_name".into(),
                    Some(story_id.clone()),
                    Some((*owner_id).into()),
                )?;
                self.add_key_in_memory(
                    &description_key,
                    format!("QuestDialog scene {dialogue_id}."),
                    "story_description".into(),
                    Some(story_id.clone()),
                    Some((*owner_id).into()),
                )?;
                self.add_key_in_memory(
                    &chapter_key,
                    "Main".into(),
                    "chapter_name".into(),
                    Some(story_id.clone()),
                    Some((*owner_id).into()),
                )?;

                let mut nodes = BTreeMap::new();
                let mut node_ids = Vec::new();
                for (index, line) in dialogue.lines.iter().enumerate() {
                    let node_id = format!("{story_id}.line_{index}");
                    let command = line.command.as_deref().unwrap_or_default();
                    let (node_type, payload, dialogue_ref, line_ref, action) =
                        if line.kind == "command" {
                            let (node_type, payload) =
                                command_node_definition(command, &self.content.items);
                            let action = match &payload {
                                content::NodePayload::Action(action) => Some(action.clone()),
                                _ => None,
                            };
                            (node_type, Some(payload), None, None, action)
                        } else {
                            (
                                "dialogue".into(),
                                Some(content::NodePayload::Dialogue {
                                    dialogue_id: (*dialogue_id).into(),
                                    line_id: line.id.clone(),
                                }),
                                Some((*dialogue_id).into()),
                                Some(line.id.clone()),
                                None,
                            )
                        };
                    let node_name_key = if line.kind == "text" {
                        line.key
                            .clone()
                            .unwrap_or_else(|| format!("{story_id}.line_{index}"))
                    } else {
                        let key = format!("ui.story.{story_id}.line_{index}.name");
                        self.add_key_in_memory(
                            &key,
                            command.to_string(),
                            "story_node_name".into(),
                            Some(story_id.clone()),
                            Some((*owner_id).into()),
                        )?;
                        key
                    };

                    nodes.insert(
                        node_id.clone(),
                        content::StoryNode {
                            node_type,
                            name_key: node_name_key,
                            payload,
                            chapter_id: Some("main".into()),
                            dialogue_id: dialogue_ref,
                            dialogue_line_id: line_ref,
                            quest_id: None,
                            scene_id: None,
                            event_id: None,
                            action,
                            condition: None,
                            choices: Vec::new(),
                            next: Vec::new(),
                            position: [420.0, 80.0 + index as f32 * 164.0],
                        },
                    );
                    node_ids.push(node_id);
                }

                let mut connections = BTreeMap::new();
                for index in 0..node_ids.len().saturating_sub(1) {
                    let id = format!("edge.{story_id}.line_{index}");
                    connections.insert(
                        id.clone(),
                        content::ConnectionDefinition {
                            id,
                            from_node_id: node_ids[index].clone(),
                            to_node_id: node_ids[index + 1].clone(),
                            output_id: "default".into(),
                            connection_kind: "next".into(),
                            label_key: None,
                            condition: None,
                        },
                    );
                }

                self.content.stories.insert(
                    story_id.clone(),
                    content::StoryDefinition {
                        name_key,
                        description_key,
                        owner_id: (*owner_id).into(),
                        entry_node_id: node_ids.first().cloned().unwrap_or_default(),
                        chapters: vec![content::ChapterDefinition {
                            id: "main".into(),
                            name_key: chapter_key,
                            node_ids,
                        }],
                        nodes,
                        connections,
                    },
                );
                if let Some(dialogue) = self.content.dialogues.get_mut(*dialogue_id) {
                    dialogue.story_id = Some(story_id.clone());
                    dialogue.chapter_id = Some("main".into());
                }
                for metadata in self.catalog.entries.values_mut() {
                    if metadata.dialogue_id.as_deref() == Some(*dialogue_id) {
                        metadata.story_id = Some(story_id.clone());
                    }
                }
                created.push(story_id);
            }

            // Remove the same dialogue scenes from their old grouped stories.
            // Keep workbench's quest nodes because they are separate gameplay
            // content, not part of the dialogue scenes being split here.
            for (old_story_id, old_dialogues) in [
                (
                    "intro",
                    &["init_text_0", "init_text_1", "init_pre_board"][..],
                ),
                ("workbench", &["init_board", "after_workbench_repair"][..]),
                ("well", &["well_repair_quest"][..]),
            ] {
                let Some(story) = self.content.stories.get_mut(old_story_id) else {
                    continue;
                };
                let removed_nodes: BTreeSet<String> = story
                    .nodes
                    .iter()
                    .filter(|(_, node)| {
                        node.dialogue_id
                            .as_deref()
                            .map(|dialogue_id| old_dialogues.contains(&dialogue_id))
                            .unwrap_or(false)
                    })
                    .map(|(node_id, _)| node_id.clone())
                    .collect();
                for node_id in &removed_nodes {
                    story.nodes.remove(node_id);
                }
                story.connections.retain(|_, connection| {
                    !removed_nodes.contains(&connection.from_node_id)
                        && !removed_nodes.contains(&connection.to_node_id)
                });
                for chapter in &mut story.chapters {
                    chapter
                        .node_ids
                        .retain(|node_id| !removed_nodes.contains(node_id));
                }
                if removed_nodes.contains(&story.entry_node_id) {
                    story.entry_node_id = story
                        .chapters
                        .iter()
                        .flat_map(|chapter| chapter.node_ids.iter())
                        .next()
                        .cloned()
                        .unwrap_or_default();
                }
            }

            self.content.stories.retain(|story_id, story| {
                !(matches!(story_id.as_str(), "intro" | "well") && story.nodes.is_empty())
            });
            self.content
                .save(&self.root)
                .map_err(AppError::Validation)?;
            self.save()?;
            Ok(created)
        })();

        if result.is_err() {
            self.catalog = snapshot.0;
            self.source = snapshot.1;
            self.targets = snapshot.2;
            self.content = snapshot.3;
        }
        result
    }

    fn create_independent_dialogue_story(
        &mut self,
        dialogue_id: &str,
        display_name: &str,
        story_id: &str,
        owner_id: &str,
    ) -> Result<bool, AppError> {
        if self.content.stories.contains_key(story_id) {
            if let Some(dialogue) = self.content.dialogues.get_mut(dialogue_id) {
                dialogue.story_id = Some(story_id.into());
                dialogue.chapter_id = Some("main".into());
            }
            return Ok(false);
        }

        let dialogue = self
            .content
            .dialogues
            .get(dialogue_id)
            .cloned()
            .ok_or_else(|| {
                AppError::Validation(format!(
                    "cannot migrate QuestDialog scene {dialogue_id}: dialogue is missing"
                ))
            })?;
        let name_key = format!("ui.story.{story_id}.name");
        let description_key = format!("ui.story.{story_id}.description");
        let chapter_key = format!("ui.story.{story_id}.chapter.main");
        self.add_key_in_memory(
            &name_key,
            display_name.into(),
            "story_name".into(),
            Some(story_id.into()),
            Some(owner_id.into()),
        )?;
        self.add_key_in_memory(
            &description_key,
            format!("QuestDialog scene {dialogue_id}."),
            "story_description".into(),
            Some(story_id.into()),
            Some(owner_id.into()),
        )?;
        self.add_key_in_memory(
            &chapter_key,
            "Main".into(),
            "chapter_name".into(),
            Some(story_id.into()),
            Some(owner_id.into()),
        )?;

        let mut nodes = BTreeMap::new();
        let mut node_ids = Vec::new();
        for (index, line) in dialogue.lines.iter().enumerate() {
            let node_id = format!("{story_id}.line_{index}");
            let command = line.command.as_deref().unwrap_or_default();
            let (node_type, payload, dialogue_ref, line_ref, action) = if line.kind == "command" {
                let (node_type, payload) = command_node_definition(command, &self.content.items);
                let action = match &payload {
                    content::NodePayload::Action(action) => Some(action.clone()),
                    _ => None,
                };
                (node_type, Some(payload), None, None, action)
            } else {
                (
                    "dialogue".into(),
                    Some(content::NodePayload::Dialogue {
                        dialogue_id: dialogue_id.into(),
                        line_id: line.id.clone(),
                    }),
                    Some(dialogue_id.into()),
                    Some(line.id.clone()),
                    None,
                )
            };
            let node_name_key = if line.kind == "text" {
                line.key
                    .clone()
                    .unwrap_or_else(|| format!("{story_id}.line_{index}"))
            } else {
                let key = format!("ui.story.{story_id}.line_{index}.name");
                self.add_key_in_memory(
                    &key,
                    command.to_string(),
                    "story_node_name".into(),
                    Some(story_id.into()),
                    Some(owner_id.into()),
                )?;
                key
            };
            nodes.insert(
                node_id.clone(),
                content::StoryNode {
                    node_type,
                    name_key: node_name_key,
                    payload,
                    chapter_id: Some("main".into()),
                    dialogue_id: dialogue_ref,
                    dialogue_line_id: line_ref,
                    quest_id: None,
                    scene_id: None,
                    event_id: None,
                    action,
                    condition: None,
                    choices: Vec::new(),
                    next: Vec::new(),
                    position: [420.0, 80.0 + index as f32 * 164.0],
                },
            );
            node_ids.push(node_id);
        }
        let mut connections = BTreeMap::new();
        for index in 0..node_ids.len().saturating_sub(1) {
            let id = format!("edge.{story_id}.line_{index}");
            connections.insert(
                id.clone(),
                content::ConnectionDefinition {
                    id,
                    from_node_id: node_ids[index].clone(),
                    to_node_id: node_ids[index + 1].clone(),
                    output_id: "default".into(),
                    connection_kind: "next".into(),
                    label_key: None,
                    condition: None,
                },
            );
        }
        self.content.stories.insert(
            story_id.into(),
            content::StoryDefinition {
                name_key,
                description_key,
                owner_id: owner_id.into(),
                entry_node_id: node_ids.first().cloned().unwrap_or_default(),
                chapters: vec![content::ChapterDefinition {
                    id: "main".into(),
                    name_key: chapter_key,
                    node_ids,
                }],
                nodes,
                connections,
            },
        );
        if let Some(dialogue) = self.content.dialogues.get_mut(dialogue_id) {
            dialogue.story_id = Some(story_id.into());
            dialogue.chapter_id = Some("main".into());
        }
        for metadata in self.catalog.entries.values_mut() {
            if metadata.dialogue_id.as_deref() == Some(dialogue_id) {
                metadata.story_id = Some(story_id.into());
            }
        }
        Ok(true)
    }

    fn migrate_remaining_grouped_dialogue_scenes(&mut self) -> Result<Vec<String>, AppError> {
        // Blacksmith already has independent scenes; this call also removes
        // the obsolete blacksmith container once all seven are present.
        self.migrate_quest_dialog_scenes()?;
        const SCENES: &[(&str, &str, &str, &str)] = &[
            (
                "farming_0",
                "Farming: introduction",
                "story.farming",
                "farming.farming_0",
            ),
            (
                "farming_1",
                "Farming: first plot",
                "story.farming",
                "farming.farming_1",
            ),
            (
                "farming_2",
                "Farming: seeds",
                "story.farming",
                "farming.farming_2",
            ),
            (
                "farming_3",
                "Farming: quest",
                "story.farming",
                "farming.farming_3",
            ),
            (
                "farming_4",
                "Farming: follow-up",
                "story.farming",
                "farming.farming_4",
            ),
            (
                "farming_5",
                "Farming: cleared plots",
                "story.farming",
                "farming.farming_5",
            ),
            (
                "farming_bucket",
                "Farming: bucket",
                "story.farming",
                "farming.farming_bucket",
            ),
            (
                "npc_default",
                "Generic NPC: default",
                "story.generic_npc",
                "generic_npc.npc_default",
            ),
            (
                "oldman_scary",
                "Old man: first meeting",
                "story.oldman",
                "oldman.oldman_scary",
            ),
            (
                "oldman_idle",
                "Old man: idle",
                "story.oldman",
                "oldman.oldman_idle",
            ),
            (
                "oldman_city",
                "Old man: city",
                "story.oldman",
                "oldman.oldman_city",
            ),
            (
                "oldman_shrine_quest",
                "Old man: shrine quest",
                "story.oldman",
                "oldman.oldman_shrine_quest",
            ),
        ];
        let snapshot = (
            self.catalog.clone(),
            self.source.clone(),
            self.targets.clone(),
            self.content.clone(),
        );
        let result = (|| {
            let mut created = Vec::new();
            for (dialogue_id, display_name, owner_id, story_id) in SCENES {
                if self.create_independent_dialogue_story(
                    dialogue_id,
                    display_name,
                    story_id,
                    owner_id,
                )? {
                    created.push((*story_id).to_string());
                }
            }

            for (old_story_id, old_dialogues) in [
                (
                    "farming",
                    &[
                        "farming_0",
                        "farming_1",
                        "farming_2",
                        "farming_3",
                        "farming_4",
                        "farming_5",
                        "farming_bucket",
                    ][..],
                ),
                ("generic_npc", &["npc_default", "test_dialog"][..]),
                (
                    "oldman",
                    &[
                        "oldman_scary",
                        "oldman_idle",
                        "oldman_city",
                        "oldman_shrine_quest",
                    ][..],
                ),
            ] {
                let Some(story) = self.content.stories.get_mut(old_story_id) else {
                    continue;
                };
                let removed_nodes: BTreeSet<String> = story
                    .nodes
                    .iter()
                    .filter(|(_, node)| {
                        node.dialogue_id
                            .as_deref()
                            .map(|dialogue_id| old_dialogues.contains(&dialogue_id))
                            .unwrap_or(false)
                    })
                    .map(|(node_id, _)| node_id.clone())
                    .collect();
                for node_id in &removed_nodes {
                    story.nodes.remove(node_id);
                }
                story.connections.retain(|_, connection| {
                    !removed_nodes.contains(&connection.from_node_id)
                        && !removed_nodes.contains(&connection.to_node_id)
                });
                for chapter in &mut story.chapters {
                    chapter
                        .node_ids
                        .retain(|node_id| !removed_nodes.contains(node_id));
                }
            }

            // Split the remaining quest-only nodes too. The old grouped
            // stories are not part of the authoring format anymore; each
            // executable piece gets its own story while keeping its node ID.
            for (old_story_id, node_id, story_id, display_name) in [
                (
                    "blacksmith",
                    "blacksmith.quest_hammer",
                    "blacksmith.quest_hammer",
                    "Craft the hammer",
                ),
                (
                    "blacksmith",
                    "blacksmith.quest_house",
                    "blacksmith.quest_house",
                    "Repair the house",
                ),
                (
                    "blacksmith",
                    "blacksmith.quest_save",
                    "blacksmith.quest_save",
                    "Save the blacksmith",
                ),
                (
                    "farming",
                    "farming.quest_craft_bucket",
                    "farming.quest_craft_bucket",
                    "Craft the bucket",
                ),
                (
                    "farming",
                    "farming.quest_farming",
                    "farming.quest_farming",
                    "Farming quest",
                ),
                (
                    "farming",
                    "farming.quest_shovel_pots",
                    "farming.quest_shovel_pots",
                    "Farming: shovel plots",
                ),
                ("oldman", "oldman.quest", "oldman.quest", "Old man quest"),
                (
                    "workbench",
                    "workbench.quest_craft_hammer",
                    "workbench.quest_craft_hammer",
                    "Craft the hammer",
                ),
                (
                    "workbench",
                    "workbench.quest_repair_entity",
                    "workbench.quest_repair_entity",
                    "Repair the workbench",
                ),
            ] {
                if self.content.stories.contains_key(story_id) {
                    continue;
                }
                let (mut node, owner_id) =
                    if let Some(source_story) = self.content.stories.get(old_story_id) {
                        let Some(node) = source_story.nodes.get(node_id).cloned() else {
                            continue;
                        };
                        (node, source_story.owner_id.clone())
                    } else if matches!(
                        old_story_id,
                        "blacksmith" | "farming" | "oldman" | "workbench"
                    ) {
                        let quest_id = match node_id {
                            "blacksmith.quest_hammer" => "npc_hammer",
                            "blacksmith.quest_house" => "repair_house",
                            "blacksmith.quest_save" => "save_blacksmith",
                            "farming.quest_craft_bucket" => "craft_bucket",
                            "farming.quest_farming" => "farming",
                            "farming.quest_shovel_pots" => "shovel_pots",
                            "oldman.quest" => "oldman_shrine",
                            "workbench.quest_craft_hammer" => "craft_hammer",
                            "workbench.quest_repair_entity" => "repair_entity",
                            _ => continue,
                        };
                        (
                            content::StoryNode {
                                node_type: "quest_start".into(),
                                name_key: String::new(),
                                payload: Some(content::NodePayload::QuestStart {
                                    quest_id: quest_id.into(),
                                }),
                                chapter_id: None,
                                dialogue_id: None,
                                dialogue_line_id: None,
                                quest_id: Some(quest_id.into()),
                                scene_id: None,
                                event_id: None,
                                action: None,
                                condition: None,
                                choices: Vec::new(),
                                next: Vec::new(),
                                position: [420.0, 80.0],
                            },
                            "story.blacksmith".into(),
                        )
                    } else {
                        continue;
                    };
                let name_key = format!("ui.story.{story_id}.name");
                let description_key = format!("ui.story.{story_id}.description");
                let chapter_key = format!("ui.story.{story_id}.chapter.main");
                self.add_key_in_memory(
                    &name_key,
                    display_name.into(),
                    "story_name".into(),
                    Some(story_id.into()),
                    Some(owner_id.clone()),
                )?;
                self.add_key_in_memory(
                    &description_key,
                    format!("Standalone content story for {node_id}."),
                    "story_description".into(),
                    Some(story_id.into()),
                    Some(owner_id.clone()),
                )?;
                self.add_key_in_memory(
                    &chapter_key,
                    "Main".into(),
                    "chapter_name".into(),
                    Some(story_id.into()),
                    Some(owner_id.clone()),
                )?;
                node.chapter_id = Some("main".into());
                self.content.stories.insert(
                    story_id.into(),
                    content::StoryDefinition {
                        name_key,
                        description_key,
                        owner_id,
                        entry_node_id: node_id.into(),
                        chapters: vec![content::ChapterDefinition {
                            id: "main".into(),
                            name_key: chapter_key,
                            node_ids: vec![node_id.into()],
                        }],
                        nodes: BTreeMap::from([(node_id.into(), node)]),
                        connections: BTreeMap::new(),
                    },
                );
                created.push(story_id.to_string());
            }
            for (quest_id, story_id) in [
                ("craft_bucket", "farming.quest_craft_bucket"),
                ("farming", "farming.quest_farming"),
                ("shovel_pots", "farming.quest_shovel_pots"),
                ("npc_hammer", "blacksmith.quest_hammer"),
                ("repair_house", "blacksmith.quest_house"),
                ("save_blacksmith", "blacksmith.quest_save"),
                ("oldman_shrine", "oldman.quest"),
                ("craft_hammer", "workbench.quest_craft_hammer"),
                ("repair_entity", "workbench.quest_repair_entity"),
            ] {
                if let Some(quest) = self.content.quests.get_mut(quest_id) {
                    quest.story_id = Some(story_id.into());
                    quest.chapter_id = Some("main".into());
                }
            }
            for old_story_id in [
                "blacksmith",
                "farming",
                "generic_npc",
                "oldman",
                "workbench",
            ] {
                self.content.stories.remove(old_story_id);
            }
            self.content
                .save(&self.root)
                .map_err(AppError::Validation)?;
            self.save()?;
            Ok(created)
        })();
        if result.is_err() {
            self.catalog = snapshot.0;
            self.source = snapshot.1;
            self.targets = snapshot.2;
            self.content = snapshot.3;
        }
        result
    }

    fn create_node(
        &mut self,
        story_id: &str,
        node_type: &str,
        speaker: &str,
        source_text: &str,
        action_type: Option<&str>,
        auto_connect_from: Option<(String, String, String)>,
    ) -> Result<CreatedNode, AppError> {
        let snapshot = (
            self.catalog.clone(),
            self.source.clone(),
            self.targets.clone(),
            self.content.clone(),
        );
        let result = (|| {
            let (chapter_id, owner_id) = {
                let story = self
                    .content
                    .stories
                    .get(story_id)
                    .ok_or_else(|| AppError::Validation(format!("unknown story {story_id}")))?;
                (
                    story
                        .chapters
                        .first()
                        .map(|chapter| chapter.id.clone())
                        .unwrap_or_else(|| "main".into()),
                    if story.owner_id.is_empty() {
                        "content.author".into()
                    } else {
                        story.owner_id.clone()
                    },
                )
            };
            let node_id = next_content_id(
                &format!("{story_id}.node"),
                self.content.stories[story_id].nodes.keys(),
            );
            let short_id = node_id
                .strip_prefix(&format!("{story_id}."))
                .unwrap_or(node_id.as_str())
                .replace('.', "_");
            let name_key = format!("story.{story_id}.node.{short_id}.name");
            self.add_key_in_memory(
                &name_key,
                format!("New {} node", node_type.replace('_', " ")),
                "story_node_name".into(),
                Some(story_id.into()),
                Some(owner_id.clone()),
            )?;

            let mut dialogue_id = None;
            let mut dialogue_line_id = None;
            let mut line_key = None;
            if node_type == "dialogue" {
                let created_dialogue_id = format!("dialogue.{story_id}.{short_id}");
                let created_line_key = format!("dialog.{story_id}.{short_id}.line");
                let line_id = format!("{created_dialogue_id}.line");
                self.add_key_in_memory(
                    &created_line_key,
                    source_text.to_string(),
                    "dialogue_line".into(),
                    Some(story_id.into()),
                    Some(owner_id.clone()),
                )?;
                self.content.dialogues.insert(
                    created_dialogue_id.clone(),
                    content::DialogueDefinition {
                        name_key: name_key.clone(),
                        story_id: Some(story_id.into()),
                        chapter_id: Some(chapter_id.clone()),
                        owner_id: Some(owner_id.clone()),
                        lines: vec![content::DialogueLineDefinition {
                            id: line_id,
                            kind: "text".into(),
                            key: Some(created_line_key.clone()),
                            command: None,
                            speaker: Some(speaker.into()),
                            variables: placeholders(source_text).into_iter().collect(),
                            on_enter: None,
                            on_exit: None,
                        }],
                        on_enter: Vec::new(),
                        on_exit: Vec::new(),
                    },
                );
                dialogue_id = Some(created_dialogue_id);
                dialogue_line_id = Some(format!("dialogue.{story_id}.{short_id}.line"));
                line_key = Some(created_line_key);
            }

            let typed_action = if node_type == "action" {
                content::action_from_type(action_type.unwrap_or("camera_shake_start"))
            } else {
                None
            };
            if node_type == "action" && typed_action.is_none() {
                return Err(AppError::Validation(format!(
                    "unsupported typed action {}",
                    action_type.unwrap_or("<missing>")
                )));
            }
            if node_type == "show_item" {
                let item_id = action_type.unwrap_or_default();
                if item_id.is_empty() || !self.content.items.contains_key(item_id) {
                    return Err(AppError::Validation(format!(
                        "show_item requires an existing item, got {item_id}"
                    )));
                }
            }
            let payload = if node_type == "dialogue" {
                Some(content::NodePayload::Dialogue {
                    dialogue_id: dialogue_id.clone().unwrap_or_default(),
                    line_id: dialogue_line_id.clone().unwrap_or_default(),
                })
            } else if node_type == "action" {
                typed_action.clone().map(content::NodePayload::Action)
            } else if node_type == "show_item" {
                Some(content::NodePayload::ShowItem {
                    item_id: action_type.unwrap_or("HAMMER").into(),
                })
            } else if node_type == "show_icon" {
                Some(content::NodePayload::ShowIcon {
                    icon_id: action_type.unwrap_or("LOVE").into(),
                })
            } else if node_type == "condition" {
                Some(content::NodePayload::Condition(
                    content::ConditionDefinition {
                        condition_type: "always".into(),
                        params: Default::default(),
                        callback_id: None,
                    },
                ))
            } else if node_type == "end" {
                Some(content::NodePayload::End)
            } else {
                None
            };
            let node = content::StoryNode {
                node_type: node_type.into(),
                name_key,
                chapter_id: Some(chapter_id.clone()),
                dialogue_id: dialogue_id.clone(),
                dialogue_line_id: dialogue_line_id.clone(),
                quest_id: None,
                scene_id: None,
                event_id: None,
                action: typed_action.clone(),
                payload,
                condition: (node_type == "condition").then(|| content::ConditionDefinition {
                    condition_type: "always".into(),
                    params: Default::default(),
                    callback_id: None,
                }),
                choices: Vec::new(),
                next: Vec::new(),
                position: [120.0, 220.0],
            };
            let story = self.content.stories.get_mut(story_id).unwrap();
            story.nodes.insert(node_id.clone(), node);
            if let Some(chapter) = story
                .chapters
                .iter_mut()
                .find(|chapter| chapter.id == chapter_id)
            {
                chapter.node_ids.push(node_id.clone());
            } else {
                story.chapters.push(content::ChapterDefinition {
                    id: chapter_id,
                    name_key: format!("story.{story_id}.chapter.main"),
                    node_ids: vec![node_id.clone()],
                });
            }

            let connection_id =
                if let Some((from_node_id, output_id, connection_kind)) = auto_connect_from {
                    Some(self.connect_nodes(
                        story_id,
                        ConnectionRequest {
                            from_node_id,
                            to_node_id: node_id.clone(),
                            output_id,
                            connection_kind,
                            label_key: None,
                            condition: None,
                        },
                    )?)
                } else {
                    None
                };
            Ok(CreatedNode {
                story_id: story_id.into(),
                node_id,
                dialogue_id,
                line_key,
                connection_id,
            })
        })();
        if result.is_err() {
            self.catalog = snapshot.0;
            self.source = snapshot.1;
            self.targets = snapshot.2;
            self.content = snapshot.3;
        }
        result
    }

    fn connect_nodes(
        &mut self,
        story_id: &str,
        request: ConnectionRequest,
    ) -> Result<String, AppError> {
        let story = self
            .content
            .stories
            .get_mut(story_id)
            .ok_or_else(|| AppError::Validation(format!("unknown story {story_id}")))?;
        if !story.nodes.contains_key(&request.from_node_id) {
            return Err(AppError::Validation(format!(
                "unknown source node {}",
                request.from_node_id
            )));
        }
        if !story.nodes.contains_key(&request.to_node_id) {
            return Err(AppError::Validation(format!(
                "unknown target node {}",
                request.to_node_id
            )));
        }
        if story.nodes[&request.from_node_id].node_type == "end" {
            return Err(AppError::Validation(
                "end nodes cannot have outgoing connections".into(),
            ));
        }
        if story.connections.values().any(|connection| {
            connection.from_node_id == request.from_node_id
                && connection.output_id == request.output_id
                && connection.to_node_id == request.to_node_id
        }) {
            return Err(AppError::Validation("duplicate connection".into()));
        }
        let connection_id = next_content_id("edge", story.connections.keys());
        story.connections.insert(
            connection_id.clone(),
            content::ConnectionDefinition {
                id: connection_id.clone(),
                from_node_id: request.from_node_id,
                to_node_id: request.to_node_id,
                output_id: request.output_id,
                connection_kind: request.connection_kind,
                label_key: request.label_key,
                condition: request.condition,
            },
        );
        Ok(connection_id)
    }

    fn update_connection(
        &mut self,
        story_id: &str,
        connection_id: &str,
        request: ConnectionRequest,
    ) -> Result<(), AppError> {
        let story = self
            .content
            .stories
            .get_mut(story_id)
            .ok_or_else(|| AppError::Validation(format!("unknown story {story_id}")))?;
        if !story.nodes.contains_key(&request.from_node_id)
            || !story.nodes.contains_key(&request.to_node_id)
        {
            return Err(AppError::Validation(
                "connection references an unknown node".into(),
            ));
        }
        if story.connections.values().any(|connection| {
            connection.id != connection_id
                && connection.from_node_id == request.from_node_id
                && connection.output_id == request.output_id
                && connection.to_node_id == request.to_node_id
        }) {
            return Err(AppError::Validation("duplicate connection".into()));
        }
        let connection = story
            .connections
            .get_mut(connection_id)
            .ok_or_else(|| AppError::Validation(format!("unknown connection {connection_id}")))?;
        connection.from_node_id = request.from_node_id;
        connection.to_node_id = request.to_node_id;
        connection.output_id = request.output_id;
        connection.connection_kind = request.connection_kind;
        connection.label_key = request.label_key;
        connection.condition = request.condition;
        Ok(())
    }

    fn delete_connection(&mut self, story_id: &str, connection_id: &str) -> Result<(), AppError> {
        let story = self
            .content
            .stories
            .get_mut(story_id)
            .ok_or_else(|| AppError::Validation(format!("unknown story {story_id}")))?;
        if story.connections.remove(connection_id).is_none() {
            return Err(AppError::Validation(format!(
                "unknown connection {connection_id}"
            )));
        }
        Ok(())
    }

    fn delete_node(&mut self, story_id: &str, node_id: &str) -> Result<(), AppError> {
        let story = self
            .content
            .stories
            .get_mut(story_id)
            .ok_or_else(|| AppError::Validation(format!("unknown story {story_id}")))?;
        if story.entry_node_id == node_id {
            return Err(AppError::Validation(
                "the story entry node cannot be deleted".into(),
            ));
        }
        if story.nodes.remove(node_id).is_none() {
            return Err(AppError::Validation(format!("unknown node {node_id}")));
        }
        story.connections.retain(|_, connection| {
            connection.from_node_id != node_id && connection.to_node_id != node_id
        });
        for chapter in &mut story.chapters {
            chapter.node_ids.retain(|id| id != node_id);
        }
        Ok(())
    }

    fn update_node(
        &mut self,
        story_id: &str,
        node_id: &str,
        name_key: String,
        node_type: String,
    ) -> Result<(), AppError> {
        if ![
            "dialogue",
            "choice",
            "condition",
            "action",
            "quest_start",
            "quest_update",
            "scene_transition",
            "event",
            "end",
        ]
        .contains(&node_type.as_str())
        {
            return Err(AppError::Validation(format!(
                "unknown node type {node_type}"
            )));
        }
        let story = self
            .content
            .stories
            .get_mut(story_id)
            .ok_or_else(|| AppError::Validation(format!("unknown story {story_id}")))?;
        let node = story
            .nodes
            .get_mut(node_id)
            .ok_or_else(|| AppError::Validation(format!("unknown node {node_id}")))?;
        if let Some(payload) = &node.payload {
            let compatible = matches!(
                (node_type.as_str(), payload),
                ("dialogue", content::NodePayload::Dialogue { .. })
                    | ("show_item", content::NodePayload::ShowItem { .. })
                    | ("show_icon", content::NodePayload::ShowIcon { .. })
                    | ("choice", content::NodePayload::Choice)
                    | ("condition", content::NodePayload::Condition(_))
                    | ("action", content::NodePayload::Action(_))
                    | ("quest_start", content::NodePayload::QuestStart { .. })
                    | ("quest_update", content::NodePayload::QuestUpdate { .. })
                    | (
                        "scene_transition",
                        content::NodePayload::SceneTransition { .. }
                    )
                    | ("event", content::NodePayload::Event { .. })
                    | ("end", content::NodePayload::End)
            );
            if !compatible {
                return Err(AppError::Validation(
                    "node type cannot change without recreating its typed payload".into(),
                ));
            }
        }
        node.name_key = name_key;
        node.node_type = node_type;
        Ok(())
    }

    fn rename_key(&mut self, old_key: &str, new_key: &str) -> Result<(), AppError> {
        if !self.source.entries.contains_key(old_key) {
            return Err(AppError::Validation(format!("unknown key {old_key}")));
        }
        if self.source.entries.contains_key(new_key) {
            return Err(AppError::Validation(format!(
                "key already exists: {new_key}"
            )));
        }
        let source = self.source.entries.remove(old_key).unwrap();
        self.source.entries.insert(new_key.to_string(), source);
        let metadata = self.catalog.entries.remove(old_key).unwrap();
        self.catalog.entries.insert(new_key.to_string(), metadata);
        for file in self.targets.values_mut() {
            if let Some(entry) = file.entries.remove(old_key) {
                file.entries.insert(new_key.to_string(), entry);
            }
        }
        for path in [self.paths.dialogues.clone(), self.paths.quests.clone()] {
            if path.exists() {
                let content = fs::read_to_string(&path)?;
                let old_json_key = format!("\"{old_key}\"");
                let new_json_key = format!("\"{new_key}\"");
                fs::write(path, content.replace(&old_json_key, &new_json_key))?;
            }
        }
        self.save()
    }

    fn export(&self) -> Result<(), AppError> {
        self.validate()?;
        let runtime = self.paths.export.clone();
        fs::create_dir_all(&runtime)?;
        let source = RuntimeFile {
            schema_version: CATALOG_SCHEMA_VERSION,
            locale: self.catalog.source_locale.clone(),
            entries: self
                .source
                .entries
                .iter()
                .map(|(key, entry)| (key.clone(), entry.text.clone()))
                .collect(),
        };
        write_json(
            &runtime.join(format!("{}.json", self.catalog.source_locale)),
            &source,
        )?;
        for (locale, file) in &self.targets {
            let entries = file
                .entries
                .iter()
                .filter(|(_, entry)| entry.status == ACCEPTED)
                .map(|(key, entry)| (key.clone(), entry.text.clone()))
                .collect();
            write_json(
                &runtime.join(format!("{locale}.json")),
                &RuntimeFile {
                    schema_version: CATALOG_SCHEMA_VERSION,
                    locale: locale.clone(),
                    entries,
                },
            )?;
        }
        self.content
            .save_runtime_at(&runtime)
            .map_err(AppError::Validation)?;
        Ok(())
    }
}

fn infer_type(key: &str) -> String {
    if key.starts_with("dialog.") {
        "dialogue_line".into()
    } else if key.starts_with("quest.") {
        "quest_description".into()
    } else {
        "ui".into()
    }
}

fn next_content_id<'a>(prefix: &str, ids: impl Iterator<Item = &'a String>) -> String {
    let used: BTreeSet<String> = ids.cloned().collect();
    let mut index = 1;
    loop {
        let candidate = format!("{prefix}_{index}");
        if !used.contains(&candidate) {
            return candidate;
        }
        index += 1;
    }
}

#[cfg(feature = "legacy-egui")]
fn edit_text(ui: &mut egui::Ui, label: &str, value: &mut String, dirty: &mut bool) {
    ui.horizontal(|ui| {
        ui.label(label);
        if ui.text_edit_singleline(value).changed() {
            *dirty = true;
        }
    });
}

fn placeholders(text: &str) -> BTreeSet<String> {
    let mut result = BTreeSet::new();
    let mut current = String::new();
    let mut inside = false;
    for character in text.chars() {
        if character == '{' {
            inside = true;
            current.clear();
        } else if character == '}' && inside {
            if !current.is_empty() {
                result.insert(current.clone());
            }
            inside = false;
        } else if inside {
            current.push(character);
        }
    }
    result
}

fn commands_in_text(text: &str) -> BTreeSet<String> {
    let mut result = BTreeSet::new();
    let mut current = String::new();
    let mut inside = false;
    for character in text.chars() {
        if character == '[' {
            inside = true;
            current.clear();
        } else if character == ']' && inside {
            if !current.is_empty() {
                result.insert(format!("[{current}]"));
            }
            inside = false;
        } else if inside {
            current.push(character);
        }
    }
    result
}

fn hash_text(text: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn command_node_definition(
    command: &str,
    items: &BTreeMap<String, content::ItemDefinition>,
) -> (String, content::NodePayload) {
    let body = command.trim().trim_start_matches('[').trim_end_matches(']');
    if let Some(item_id) = body.strip_prefix("SHOW:") {
        let item_id = item_id.to_string();
        let (node_type, action) = if items.contains_key(&item_id) {
            (
                "show_item",
                content::NodePayload::ShowItem {
                    item_id: item_id.clone(),
                },
            )
        } else {
            (
                "show_icon",
                content::NodePayload::ShowIcon {
                    icon_id: item_id.clone(),
                },
            )
        };
        return (node_type.into(), action);
    }
    if body == "START_CAMERA_SHAKE" || body == "STOP_CAMERA_SHAKE" {
        return (
            "action".into(),
            content::NodePayload::Action(if body == "START_CAMERA_SHAKE" {
                content::ActionDefinition::CameraShakeStart
            } else {
                content::ActionDefinition::CameraShakeStop
            }),
        );
    }
    (
        "event".into(),
        content::NodePayload::Event {
            event_id: body.into(),
        },
    )
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, AppError> {
    let text = fs::read_to_string(path)?;
    serde_json::from_str(&text).map_err(|source| AppError::Json {
        path: path.display().to_string(),
        source,
    })
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), AppError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let text = serde_json::to_string_pretty(value).map_err(|source| AppError::Json {
        path: path.display().to_string(),
        source,
    })?;
    fs::write(path, format!("{text}\n"))?;
    Ok(())
}

#[derive(Parser)]
#[command(
    name = "kstory",
    about = "Edit, validate and export portable KStory projects"
)]
struct Cli {
    #[arg(
        long = "project",
        alias = "root",
        short = 'p',
        global = true,
        default_value = ".",
        value_name = "PROJECT"
    )]
    root: PathBuf,
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    Sync,
    Validate,
    Export,
    List {
        #[arg(long)]
        locale: String,
        #[arg(long)]
        status: String,
    },
    Accept {
        #[arg(long)]
        locale: String,
        #[arg(long)]
        key: Option<String>,
        #[arg(long)]
        story: Option<String>,
        #[arg(long)]
        all_on_review: bool,
    },
    Reject {
        #[arg(long)]
        locale: String,
        #[arg(long)]
        key: String,
    },
    SetTranslation {
        #[arg(long)]
        locale: String,
        #[arg(long)]
        key: String,
        #[arg(long)]
        text: String,
        #[arg(long, default_value = "llm")]
        translated_by: String,
    },
    AddKey {
        #[arg(long)]
        key: String,
        #[arg(long)]
        text: String,
        #[arg(long, default_value = "ui")]
        entry_type: String,
        #[arg(long)]
        story: Option<String>,
        #[arg(long)]
        owner: Option<String>,
    },
    RenameKey {
        #[arg(long)]
        old: String,
        #[arg(long)]
        new: String,
    },
    MigrateQuestDialogScenes,
    MigrateOpeningQuestDialogScenes,
    MigrateTypedContent,
    Open {
        path: PathBuf,
    },
    Gui,
}

fn main() -> Result<(), AppError> {
    let cli = Cli::parse();
    let command = cli.command.unwrap_or(Command::Gui);
    match command {
        Command::Gui => run_gui(cli.root),
        Command::Open { path } => run_gui(path),
        Command::Sync => {
            let mut store = Store::load(cli.root)?;
            println!("{}", serde_json::to_string_pretty(&store.sync()?).unwrap());
            Ok(())
        }
        Command::Validate => {
            let store = Store::load(cli.root)?;
            store
                .validate()
                .map(|_| println!("catalog valid"))
                .map_err(|error| {
                    eprintln!("{error}");
                    error
                })
        }
        Command::Export => {
            let store = Store::load(cli.root)?;
            store.export()?;
            println!("runtime bundles exported");
            Ok(())
        }
        Command::List { locale, status } => {
            let store = Store::load(cli.root)?;
            for (key, text) in store.list(&locale, &status) {
                println!("{key}\t{text}");
            }
            Ok(())
        }
        Command::Accept {
            locale,
            key,
            story,
            all_on_review,
        } => {
            let mut store = Store::load(cli.root)?;
            if all_on_review {
                println!("accepted {} entries", store.accept_all_on_review(&locale)?);
            } else if let Some(key) = key {
                store.accept(&locale, &key)?;
            } else if let Some(story) = story {
                let keys: Vec<String> = store
                    .catalog
                    .entries
                    .iter()
                    .filter(|(_, metadata)| metadata.story_id.as_deref() == Some(story.as_str()))
                    .map(|(key, _)| key.clone())
                    .collect();
                let mut accepted = 0;
                let mut skipped = 0;
                for key in keys {
                    if store.accept(&locale, &key).is_ok() {
                        accepted += 1;
                    } else {
                        skipped += 1;
                    }
                }
                println!(
                    "accepted {accepted} entries; skipped {skipped} invalid or pending entries"
                );
            } else {
                return Err(AppError::Validation(
                    "provide --key, --story, or --all-on-review".into(),
                ));
            }
            Ok(())
        }
        Command::Reject { locale, key } => {
            let mut store = Store::load(cli.root)?;
            store.reject(&locale, &key)
        }
        Command::MigrateQuestDialogScenes => {
            let mut store = Store::load(cli.root)?;
            let created = store.migrate_quest_dialog_scenes()?;
            if created.is_empty() {
                println!("QuestDialog scenes are already migrated");
            } else {
                println!("created {} independent scenes:", created.len());
                for story_id in created {
                    println!("- {story_id}");
                }
            }
            Ok(())
        }
        Command::MigrateOpeningQuestDialogScenes => {
            let mut store = Store::load(cli.root)?;
            let mut created = store.migrate_opening_quest_dialog_scenes()?;
            created.extend(store.migrate_remaining_grouped_dialogue_scenes()?);
            if created.is_empty() {
                println!("Grouped QuestDialog scenes are already migrated");
            } else {
                println!("created {} independent scenes:", created.len());
                for story_id in created {
                    println!("- {story_id}");
                }
            }
            Ok(())
        }
        Command::MigrateTypedContent => {
            let store = Store::load(cli.root)?;
            store.save_content()?;
            println!("content migrated to typed command nodes");
            Ok(())
        }
        Command::SetTranslation {
            locale,
            key,
            text,
            translated_by,
        } => {
            let mut store = Store::load(cli.root)?;
            let _ = store.sync()?;
            store.set_translation(&locale, &key, text, translated_by)
        }
        Command::AddKey {
            key,
            text,
            entry_type,
            story,
            owner,
        } => {
            let mut store = Store::load(cli.root)?;
            let _ = store.sync()?;
            store.add_key(&key, text, entry_type, story, owner)
        }
        Command::RenameKey { old, new } => {
            let mut store = Store::load(cli.root)?;
            store.rename_key(&old, &new)
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum SortMode {
    #[default]
    Key,
    Status,
    Story,
    Owner,
}

impl SortMode {
    fn label(self) -> &'static str {
        match self {
            Self::Key => "Key",
            Self::Status => "Status",
            Self::Story => "Story",
            Self::Owner => "Owner",
        }
    }
}

#[derive(Debug, Clone, Default)]
struct FilterState {
    search: String,
    status: String,
    entry_type: String,
    story: String,
    quest: String,
    owner: String,
    only_orphaned: bool,
    sort_mode: SortMode,
}

impl FilterState {
    fn clear(&mut self) {
        self.search.clear();
        self.status.clear();
        self.entry_type.clear();
        self.story.clear();
        self.quest.clear();
        self.owner.clear();
        self.only_orphaned = false;
        self.sort_mode = SortMode::Key;
    }

    fn is_active(&self) -> bool {
        !self.search.is_empty()
            || !self.status.is_empty()
            || !self.entry_type.is_empty()
            || !self.story.is_empty()
            || !self.quest.is_empty()
            || !self.owner.is_empty()
            || self.only_orphaned
    }
}

#[derive(Debug, Clone, PartialEq)]
struct EntryViewModel {
    key: String,
    source_text: String,
    target_text: String,
    status: String,
    metadata: KeyEntry,
    source_hash: String,
    stored_source_hash: String,
    text_hash: String,
    translated_by: Option<String>,
    validation_errors: Vec<String>,
}

#[cfg(feature = "legacy-egui")]
mod legacy_egui {
    use super::*;
    use eframe::egui;
    use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
    use std::time::{Duration, Instant};

    #[derive(Debug, Clone)]
    pub(crate) struct UiNotification {
        message: String,
        is_error: bool,
        expires_at: Instant,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum EditorAction {
        Save,
        MarkOnReview,
        Accept,
        Reject,
        Reset,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum ContextAction {
        MarkWaiting,
        MarkOnReview,
        Accept,
        Reject,
        ShowUsages,
        ShowMetadata,
    }

    fn run_gui(root: PathBuf) -> Result<(), AppError> {
        dioxus_ui::run(root)
    }

    fn configure_theme(context: &egui::Context) {
        let mut style = (*context.style()).clone();
        style.spacing.item_spacing = egui::vec2(8.0, 6.0);
        style.spacing.button_padding = egui::vec2(10.0, 6.0);
        style.visuals.window_fill = egui::Color32::from_rgb(27, 31, 38);
        style.visuals.panel_fill = egui::Color32::from_rgb(22, 26, 32);
        style.visuals.faint_bg_color = egui::Color32::from_rgb(33, 38, 47);
        style.visuals.extreme_bg_color = egui::Color32::from_rgb(15, 18, 22);
        style.visuals.override_text_color = Some(egui::Color32::from_rgb(226, 232, 240));
        style.visuals.widgets.inactive.bg_fill = egui::Color32::from_rgb(52, 62, 76);
        style.visuals.widgets.inactive.fg_stroke.color = egui::Color32::from_rgb(218, 226, 238);
        style.visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(67, 83, 103);
        style.visuals.widgets.hovered.fg_stroke.color = egui::Color32::WHITE;
        style.visuals.widgets.active.bg_fill = egui::Color32::from_rgb(48, 106, 160);
        style.visuals.widgets.active.fg_stroke.color = egui::Color32::WHITE;
        style.visuals.selection.bg_fill = egui::Color32::from_rgb(45, 96, 145);
        style.spacing.scroll = egui::style::ScrollStyle {
            floating: false,
            bar_width: 10.0,
            handle_min_length: 32.0,
            bar_inner_margin: 3.0,
            bar_outer_margin: 2.0,
            foreground_color: true,
            ..egui::style::ScrollStyle::solid()
        };
        context.set_style(style);
    }

    fn app_background() -> egui::Color32 {
        egui::Color32::from_rgb(24, 30, 38)
    }

    fn panel_background() -> egui::Color32 {
        egui::Color32::from_rgb(27, 33, 42)
    }

    fn toolbar_background() -> egui::Color32 {
        egui::Color32::from_rgb(34, 41, 51)
    }

    fn panel_frame(fill: egui::Color32, margin: f32) -> egui::Frame {
        egui::Frame::default()
            .fill(fill)
            .inner_margin(egui::Margin::same(margin))
    }

    /// Keep a side panel's inner UI from growing the panel because of a long key,
    /// story id, or an unbounded horizontal row. `SidePanel::exact_width` controls
    /// the requested panel size, but egui can still expand the stored panel rect
    /// when a child reports a larger natural width. All side-panel content must be
    /// bounded before adding widgets.
    fn constrain_side_panel_content(ui: &mut egui::Ui) {
        // The panel width is now user-resizable. Lock the contents to the width
        // selected by the user so long IDs/text cannot resize it a second time.
        let available_width = ui.available_width().max(0.0);
        ui.set_width(available_width);
    }

    fn toolbar_row(ui: &mut egui::Ui, height: f32, add_contents: impl FnOnce(&mut egui::Ui)) {
        let width = ui.available_width().max(0.0);
        ui.allocate_ui_with_layout(
            egui::vec2(width, height),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                add_contents(ui);
            },
        );
    }

    fn status_color(status: &str) -> egui::Color32 {
        match status {
            WAITING => egui::Color32::from_rgb(236, 173, 72),
            ON_REVIEW => egui::Color32::from_rgb(83, 169, 230),
            ACCEPTED => egui::Color32::from_rgb(92, 195, 124),
            "source" => egui::Color32::from_rgb(160, 180, 205),
            _ => egui::Color32::LIGHT_GRAY,
        }
    }

    fn status_label(ui: &mut egui::Ui, status: &str) {
        ui.colored_label(
            status_color(status),
            format!("● {}", status_display(status)),
        );
    }

    fn status_display(status: &str) -> String {
        status.replace('_', " ")
    }

    pub(crate) struct EditorApp {
        pub(crate) root: PathBuf,
        pub(crate) store: Store,
        pub(crate) locale: String,
        pub(crate) filters: FilterState,
        pub(crate) show_content: bool,
        pub(crate) show_studio: bool,
        pub(crate) show_inspector: bool,
        pub(crate) show_metadata: bool,
        pub(crate) selected: Option<String>,
        pub(crate) draft: String,
        pub(crate) draft_dirty: bool,
        pub(crate) notification: Option<UiNotification>,
        pub(crate) external_change: std::sync::mpsc::Receiver<bool>,
        pub(crate) external_snapshot: Option<Store>,
        pub(crate) external_conflict: bool,
        pub(crate) show_compare: bool,
        pub(crate) confirm_reset: bool,
        pub(crate) selected_story_id: Option<String>,
        pub(crate) selected_node_id: Option<String>,
        pub(crate) content_search: String,
        pub(crate) content_view: ContentView,
        pub(crate) content_dirty: bool,
        pub(crate) content_section: ContentSection,
        pub(crate) selected_content_id: Option<String>,
        pub(crate) reader: StoryReaderState,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub(crate) enum ContentSection {
        Stories,
        Quests,
        Dialogues,
        Items,
        Entities,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub(crate) enum ContentView {
        Graph,
        Sequence,
        Reader,
    }

    #[derive(Debug, Clone, Default, PartialEq, Eq)]
    pub(crate) struct StoryReaderState {
        pub(crate) story_id: Option<String>,
        pub(crate) current_node_id: Option<String>,
        pub(crate) history: Vec<String>,
        pub(crate) selected_choice: Option<String>,
        pub(crate) locale: String,
        pub(crate) active: bool,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub(crate) struct LocalizedPreview {
        pub(crate) text: String,
        pub(crate) used_fallback: bool,
        pub(crate) status: String,
    }

    impl StoryReaderState {
        pub(crate) fn restart_at(
            &mut self,
            story_id: String,
            entry_node_id: String,
            locale: String,
        ) {
            self.story_id = Some(story_id);
            self.current_node_id = Some(entry_node_id);
            self.history.clear();
            self.selected_choice = None;
            self.locale = locale;
            self.active = true;
        }

        pub(crate) fn back(&mut self) {
            if let Some(previous) = self.history.pop() {
                self.current_node_id = Some(previous);
                self.selected_choice = None;
            }
        }

        pub(crate) fn advance_to(&mut self, next: String) {
            if let Some(current) = self.current_node_id.take() {
                self.history.push(current);
            }
            self.current_node_id = Some(next);
            self.selected_choice = None;
        }
    }

    impl EditorApp {
        pub(crate) fn request_selection(&mut self, key: String) {
            if self.selected.as_deref() == Some(key.as_str()) {
                return;
            }
            if self.draft_dirty {
                self.notify(
                    "Save or reset the current draft before changing selection.",
                    true,
                );
                return;
            }
            self.selected = Some(key);
            self.load_draft();
        }

        pub(crate) fn load_draft(&mut self) {
            self.draft = self
                .selected
                .as_deref()
                .and_then(|key| {
                    if self.locale == self.store.catalog.source_locale {
                        self.store.source.entries.get(key)
                    } else {
                        self.store
                            .targets
                            .get(&self.locale)
                            .and_then(|file| file.entries.get(key))
                    }
                })
                .map(|entry| entry.text.clone())
                .unwrap_or_default();
            self.draft_dirty = false;
        }

        fn notify(&mut self, message: impl Into<String>, is_error: bool) {
            self.notification = Some(UiNotification {
                message: message.into(),
                is_error,
                expires_at: Instant::now() + Duration::from_secs(6),
            });
        }

        fn selected_view(&self) -> Option<EntryViewModel> {
            self.selected
                .as_deref()
                .and_then(|key| self.store.entry_view(&self.locale, key))
        }

        fn target_status(&self, key: &str) -> &str {
            if self.locale == self.store.catalog.source_locale {
                return "source";
            }
            self.store
                .targets
                .get(&self.locale)
                .and_then(|file| file.entries.get(key))
                .map(|entry| entry.status.as_str())
                .unwrap_or(WAITING)
        }

        fn values_for(&self, field: &str) -> Vec<String> {
            let mut values = BTreeSet::new();
            for metadata in self.store.catalog.entries.values() {
                let value = match field {
                    "type" => Some(metadata.entry_type.as_str()),
                    "story" => metadata.story_id.as_deref(),
                    "quest" => metadata.quest_id.as_deref(),
                    "owner" => metadata.owner_id.as_deref(),
                    _ => None,
                };
                if let Some(value) = value.filter(|value| !value.is_empty()) {
                    values.insert(value.to_string());
                }
            }
            values.into_iter().collect()
        }

        fn apply_action(&mut self, action: EditorAction) {
            let Some(key) = self.selected.clone() else {
                self.notify("Select an entry first.", true);
                return;
            };
            let result = match action {
                EditorAction::Save => {
                    self.store
                        .save_translation(&self.locale, &key, self.draft.clone())
                }
                EditorAction::MarkOnReview => self.store.mark_on_review(&self.locale, &key),
                EditorAction::Accept => self.store.accept(&self.locale, &key),
                EditorAction::Reject => self.store.reject(&self.locale, &key),
                EditorAction::Reset => self.store.reset_translation(&self.locale, &key),
            };
            match result {
                Ok(()) => {
                    self.draft_dirty = false;
                    self.load_draft();
                    self.notify(
                        match action {
                            EditorAction::Save => {
                                "Translation saved and marked waiting for translation."
                            }
                            EditorAction::MarkOnReview => "Translation marked on review.",
                            EditorAction::Accept => "Translation accepted.",
                            EditorAction::Reject => {
                                "Translation returned to waiting for translation."
                            }
                            EditorAction::Reset => "Translation reset.",
                        },
                        false,
                    );
                }
                Err(error) => self.notify(error.to_string(), true),
            }
        }

        fn apply_context_action(&mut self, key: &str, action: ContextAction) {
            let result = match action {
                ContextAction::MarkWaiting | ContextAction::Reject => {
                    self.store.reject(&self.locale, key)
                }
                ContextAction::MarkOnReview => self.store.mark_on_review(&self.locale, key),
                ContextAction::Accept => self.store.accept(&self.locale, key),
                ContextAction::ShowUsages | ContextAction::ShowMetadata => Ok(()),
            };
            match result {
                Ok(()) => {
                    if self.selected.as_deref() == Some(key) {
                        self.load_draft();
                    }
                    self.notify("Entry updated.", false);
                }
                Err(error) => self.notify(error.to_string(), true),
            }
            if matches!(
                action,
                ContextAction::ShowUsages | ContextAction::ShowMetadata
            ) {
                self.selected = Some(key.to_string());
                self.show_inspector = true;
                if action == ContextAction::ShowMetadata {
                    self.show_metadata = true;
                }
            }
        }

        fn check_external_changes(&mut self) {
            let mut changed = false;
            while self.external_change.try_recv().is_ok() {
                changed = true;
            }
            if !changed {
                return;
            }
            match Store::load(&self.root) {
                Ok(mut fresh) => match fresh.sync() {
                    Ok(_) => {
                        if self.draft_dirty {
                            self.external_snapshot = Some(fresh);
                            self.external_conflict = true;
                        } else {
                            self.store = fresh;
                            self.load_draft();
                            self.notify("Catalog reloaded after an external change.", false);
                        }
                    }
                    Err(error) => self.notify(format!("External sync failed: {error}"), true),
                },
                Err(error) => self.notify(format!("External reload failed: {error}"), true),
            }
        }

        fn keyboard_action(&mut self, context: &egui::Context, visible_keys: &[String]) {
            let (command, shift, up, down, enter, escape) = context.input(|input| {
                (
                    input.modifiers.command,
                    input.modifiers.shift,
                    input.key_pressed(egui::Key::ArrowUp),
                    input.key_pressed(egui::Key::ArrowDown),
                    input.key_pressed(egui::Key::Enter),
                    input.key_pressed(egui::Key::Escape),
                )
            });
            if command && context.input(|input| input.key_pressed(egui::Key::F)) {
                context.memory_mut(|memory| memory.request_focus(egui::Id::new("global_search")));
            }
            if command && context.input(|input| input.key_pressed(egui::Key::S)) {
                self.apply_action(EditorAction::Save);
            } else if command && enter && shift {
                self.apply_action(EditorAction::Accept);
            } else if command && enter {
                self.apply_action(EditorAction::MarkOnReview);
            } else if escape {
                self.confirm_reset = false;
                self.external_conflict = false;
                self.show_compare = false;
            } else if !context.wants_keyboard_input() && (up || down) && !visible_keys.is_empty() {
                if self.draft_dirty {
                    self.notify(
                        "Save or reset the current draft before changing selection.",
                        true,
                    );
                    return;
                }
                let current = self
                    .selected
                    .as_ref()
                    .and_then(|key| visible_keys.iter().position(|candidate| candidate == key));
                let next = match (current, up, down) {
                    (Some(index), true, _) => index.saturating_sub(1),
                    (Some(index), _, true) => (index + 1).min(visible_keys.len() - 1),
                    (None, _, true) => 0,
                    (None, _, false) => visible_keys.len() - 1,
                    _ => 0,
                };
                self.request_selection(visible_keys[next].clone());
            } else if enter && !context.wants_keyboard_input() && self.selected.is_none() {
                if let Some(key) = visible_keys.first() {
                    self.request_selection(key.clone());
                }
            }
        }

        fn save_content(&mut self) {
            match self.store.content.save(&self.root) {
                Ok(()) => {
                    self.content_dirty = false;
                    self.notify("Content saved.", false);
                }
                Err(error) => self.notify(format!("Content save failed: {error}"), true),
            }
        }

        fn content_story(&self) -> Option<&content::StoryDefinition> {
            self.selected_story_id
                .as_ref()
                .and_then(|id| self.store.content.stories.get(id))
        }

        fn ensure_content_selection(&mut self) {
            if self.selected_story_id.is_none() {
                self.selected_story_id = self.store.content.stories.keys().next().cloned();
            }
            if let Some(story) = self.content_story() {
                if self
                    .selected_node_id
                    .as_ref()
                    .map(|node| story.nodes.contains_key(node))
                    != Some(true)
                {
                    self.selected_node_id = Some(story.entry_node_id.clone());
                }
            }
        }

        fn show_content_studio(&mut self, context: &egui::Context) {
            self.ensure_content_selection();
            egui::TopBottomPanel::top("content_studio_toolbar")
                .frame(panel_frame(panel_background(), 8.0))
                .show(context, |ui| {
                    toolbar_row(ui, 32.0, |ui| {
                        ui.label(egui::RichText::new("Content authoring").strong().size(16.0));
                        ui.separator();
                        for (section, label) in [
                            (ContentSection::Stories, "Stories"),
                            (ContentSection::Quests, "Quests"),
                            (ContentSection::Dialogues, "Dialogues"),
                            (ContentSection::Items, "Items"),
                            (ContentSection::Entities, "Entities"),
                        ] {
                            if ui
                                .selectable_label(self.content_section == section, label)
                                .clicked()
                            {
                                self.content_section = section;
                                self.selected_node_id = None;
                                self.content_view = ContentView::Graph;
                            }
                        }
                    });
                    ui.add_space(4.0);
                    toolbar_row(ui, 32.0, |ui| {
                        ui.label("View");
                        ui.separator();
                        for (view, label) in [
                            (ContentView::Graph, "Graph"),
                            (ContentView::Sequence, "Sequence"),
                            (ContentView::Reader, "Reader"),
                        ] {
                            if ui
                                .selectable_label(self.content_view == view, label)
                                .clicked()
                            {
                                self.content_view = view;
                                if view == ContentView::Reader {
                                    self.start_reader_for_selected_story();
                                }
                            }
                        }
                        ui.separator();
                        if self.content_dirty {
                            ui.colored_label(
                                egui::Color32::from_rgb(236, 173, 72),
                                "● Unsaved changes",
                            );
                        }
                        if ui.button("Save content").clicked() {
                            self.save_content();
                        }
                        if ui.button("Validate").clicked() {
                            match self.store.validate() {
                                Ok(_) => self.notify("Content catalog is valid.", false),
                                Err(error) => self.notify(error.to_string(), true),
                            }
                        }
                    });
                });

            if self.content_section != ContentSection::Stories {
                self.show_catalog_section(context);
                return;
            }

            egui::SidePanel::left("story_explorer")
                .default_width(EXPLORER_WIDTH)
                .width_range(SIDE_PANEL_MIN_WIDTH..=SIDE_PANEL_MAX_WIDTH)
                .resizable(true)
                .frame(panel_frame(panel_background(), 12.0))
                .show(context, |ui| {
                    constrain_side_panel_content(ui);
                    ui.heading("Stories");
                    ui.horizontal(|ui| {
                        // Reserve a fixed slot for the action button. Keeping the
                        // sum exact prevents this row from increasing the panel's
                        // natural width by a few pixels on the first frame.
                        let action_width = 84.0;
                        let search_width = (ui.available_width() - action_width - 8.0).max(100.0);
                        ui.add_sized(
                            [search_width, 28.0],
                            egui::TextEdit::singleline(&mut self.content_search)
                                .hint_text("Search stories, nodes, quests..."),
                        );
                        if ui
                            .add_sized([action_width, 28.0], egui::Button::new("New story"))
                            .clicked()
                        {
                            match self.store.create_story() {
                                Ok(created) => {
                                    self.selected_story_id = Some(created.story_id.clone());
                                    self.selected_node_id = Some(created.entry_node_id.clone());
                                    self.content_section = ContentSection::Stories;
                                    self.content_view = ContentView::Graph;
                                    self.content_dirty = true;
                                    self.notify(
                                        format!(
                                            "Created story {} with an initial dialogue.",
                                            created.story_id
                                        ),
                                        false,
                                    );
                                }
                                Err(error) => {
                                    self.notify(format!("New story failed: {error}"), true)
                                }
                            }
                        }
                    });
                    ui.separator();
                    egui::ScrollArea::vertical()
                        .id_salt("story_explorer_scroll")
                        .max_width(ui.available_width())
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            let search = self.content_search.to_lowercase();
                            let ids: Vec<String> = self
                                .store
                                .content
                                .stories
                                .iter()
                                .filter(|(id, story)| {
                                    search.is_empty()
                                        || id.to_lowercase().contains(&search)
                                        || story.name_key.to_lowercase().contains(&search)
                                        || story
                                            .nodes
                                            .keys()
                                            .any(|node| node.to_lowercase().contains(&search))
                                })
                                .map(|(id, _)| id.clone())
                                .collect();
                            for id in ids {
                                let selected =
                                    self.selected_story_id.as_deref() == Some(id.as_str());
                                let node_count = self
                                    .store
                                    .content
                                    .stories
                                    .get(&id)
                                    .map(|story| story.nodes.len())
                                    .unwrap_or_default();
                                if ui
                                    .add_sized(
                                        [ui.available_width(), 30.0],
                                        egui::SelectableLabel::new(
                                            selected,
                                            format!("{id}  ·  {node_count} nodes"),
                                        ),
                                    )
                                    .clicked()
                                {
                                    self.selected_story_id = Some(id);
                                    self.selected_node_id = None;
                                    self.content_view = ContentView::Graph;
                                }
                            }
                        });
                });

            if self.show_inspector {
                self.show_story_inspector(context);
            }
            egui::CentralPanel::default()
                .frame(panel_frame(app_background(), 14.0))
                .show(context, |ui| {
                    let Some(story_id) = self.selected_story_id.clone() else {
                        ui.centered_and_justified(|ui| ui.heading("Create or select a story"));
                        return;
                    };
                    let Some((node_count, entry_node_id, node_ids)) =
                        self.store.content.stories.get(&story_id).map(|story| {
                            (
                                story.nodes.len(),
                                story.entry_node_id.clone(),
                                story.nodes.keys().cloned().collect::<Vec<_>>(),
                            )
                        })
                    else {
                        return;
                    };
                    ui.horizontal(|ui| {
                        ui.heading(&story_id);
                        ui.separator();
                        ui.label(format!("{} nodes · entry: {}", node_count, entry_node_id));
                        if ui.button("Arrange vertically").clicked() {
                            self.arrange_story(&story_id);
                        }
                        if ui.button("Reset layout").clicked() {
                            self.arrange_story(&story_id);
                        }
                        if ui.button("Add node").clicked() {
                            let node_id = next_content_id("node", node_ids.iter());
                            if let Some(story) = self.store.content.stories.get_mut(&story_id) {
                                story.nodes.insert(
                                    node_id.clone(),
                                    content::StoryNode {
                                        node_type: "dialogue".into(),
                                        name_key: format!("story.{story_id}.node.{node_id}"),
                                        chapter_id: story
                                            .chapters
                                            .first()
                                            .map(|chapter| chapter.id.clone()),
                                        dialogue_id: None,
                                        dialogue_line_id: None,
                                        quest_id: None,
                                        scene_id: None,
                                        event_id: None,
                                        action: None,
                                        condition: None,
                                        choices: Vec::new(),
                                        next: Vec::new(),
                                        position: [120.0, 220.0],
                                    },
                                );
                                if let Some(chapter) = story.chapters.first_mut() {
                                    chapter.node_ids.push(node_id.clone());
                                }
                            }
                            self.selected_node_id = Some(node_id);
                            self.content_dirty = true;
                        }
                        ui.colored_label(
                            egui::Color32::from_rgb(83, 169, 230),
                            "Preview is simulated",
                        );
                    });
                    ui.separator();
                    match self.content_view {
                        ContentView::Reader => self.show_story_reader(ui, &story_id),
                        ContentView::Sequence => self.show_story_sequence(ui, &story_id),
                        ContentView::Graph => self.show_story_graph(ui, &story_id),
                    }
                });
        }

        fn show_story_graph(&mut self, ui: &mut egui::Ui, story_id: &str) {
            let Some(story) = self.store.content.stories.get(story_id) else {
                return;
            };
            let nodes: Vec<(String, content::StoryNode)> = story
                .nodes
                .iter()
                .map(|(id, node)| (id.clone(), node.clone()))
                .collect();
            let (layout, issues) = content::arrange_story_top_down(story);
            let horizontal_legacy_layout = nodes.len() > 1
                && nodes
                    .iter()
                    .all(|(_, node)| (node.position[1] - nodes[0].1.position[1]).abs() < 0.1);
            let raw_positions: BTreeMap<String, [f32; 2]> = nodes
                .iter()
                .map(|(id, node)| {
                    let position = if horizontal_legacy_layout {
                        layout
                            .get(id)
                            .map(|position| [position.x, position.y])
                            .unwrap_or(node.position)
                    } else {
                        node.position
                    };
                    (id.clone(), position)
                })
                .collect();

            // The content layout uses a deterministic coordinate system so saved
            // positions remain reproducible. Normalize that system to the visible
            // graph viewport before painting: a linear story must be centered in
            // the editor, not centered in an arbitrary 1440px canvas.
            let min_x = raw_positions
                .values()
                .map(|position| position[0])
                .fold(f32::INFINITY, f32::min);
            let max_raw_x = raw_positions
                .values()
                .map(|position| position[0] + content::NODE_WIDTH)
                .fold(0.0, f32::max);
            let content_width = (max_raw_x - min_x).max(content::NODE_WIDTH);
            let visible_width = ui.available_width().max(content::NODE_WIDTH + 32.0);
            let canvas_width = visible_width.max(content_width + 96.0);
            let x_offset = (canvas_width - content_width) / 2.0 - min_x;
            let display_positions: BTreeMap<String, [f32; 2]> = raw_positions
                .into_iter()
                .map(|(id, [x, y])| (id, [x + x_offset, y]))
                .collect();

            let mut max_x: f32 = canvas_width;
            let mut max_y: f32 = 560.0;
            for (id, node) in &nodes {
                let position = display_positions.get(id).copied().unwrap_or(node.position);
                max_x = max_x.max(position[0] + content::NODE_WIDTH + 80.0);
                max_y = max_y.max(position[1] + content::NODE_HEIGHT + 120.0);
            }
            egui::ScrollArea::both()
                .id_salt("story_graph_scroll")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    let (response, painter) =
                        ui.allocate_painter(egui::vec2(max_x, max_y), egui::Sense::hover());
                    let origin = response.rect.min;
                    if !issues.orphaned_nodes.is_empty() || !issues.cycle_edges.is_empty() {
                        let mut message = String::from("Graph issues: ");
                        if !issues.orphaned_nodes.is_empty() {
                            message.push_str(&format!("{} orphaned", issues.orphaned_nodes.len()));
                        }
                        if !issues.cycle_edges.is_empty() {
                            if !issues.orphaned_nodes.is_empty() {
                                message.push_str(", ");
                            }
                            message
                                .push_str(&format!("{} cycle edge(s)", issues.cycle_edges.len()));
                        }
                        ui.colored_label(egui::Color32::from_rgb(235, 150, 120), message);
                    }
                    for (id, node) in &nodes {
                        let position = display_positions.get(id).copied().unwrap_or(node.position);
                        let from = origin
                            + egui::vec2(
                                position[0] + content::NODE_WIDTH / 2.0,
                                position[1] + content::NODE_HEIGHT,
                            );
                        for next in &node.next {
                            if let Some((_, target)) =
                                nodes.iter().find(|(candidate, _)| candidate == next)
                            {
                                let target_position = display_positions
                                    .get(next)
                                    .copied()
                                    .unwrap_or(target.position);
                                let to = origin
                                    + egui::vec2(
                                        target_position[0] + content::NODE_WIDTH / 2.0,
                                        target_position[1],
                                    );
                                painter.line_segment(
                                    [from, to],
                                    egui::Stroke::new(2.0, egui::Color32::from_rgb(92, 130, 165)),
                                );
                            }
                        }
                        for choice in &node.choices {
                            if let Some((_, target)) = nodes
                                .iter()
                                .find(|(candidate, _)| candidate == &choice.next)
                            {
                                let target_position = display_positions
                                    .get(&choice.next)
                                    .copied()
                                    .unwrap_or(target.position);
                                let to = origin
                                    + egui::vec2(
                                        target_position[0] + content::NODE_WIDTH / 2.0,
                                        target_position[1],
                                    );
                                painter.line_segment(
                                    [from, to],
                                    egui::Stroke::new(2.0, egui::Color32::from_rgb(236, 173, 72)),
                                );
                            }
                        }
                        let rect = egui::Rect::from_min_size(
                            origin + egui::vec2(position[0], position[1]),
                            egui::vec2(content::NODE_WIDTH, content::NODE_HEIGHT),
                        );
                        let selected = self.selected_node_id.as_deref() == Some(id.as_str());
                        let [r, g, b] = content::node_type_color(&node.node_type);
                        let semantic_fill = egui::Color32::from_rgb(r, g, b);
                        let fill = if selected {
                            egui::Color32::from_rgb(45, 96, 145)
                        } else {
                            semantic_fill
                        };
                        let invalid = issues.orphaned_nodes.contains(id)
                            || issues
                                .cycle_edges
                                .iter()
                                .any(|(from, to)| from == id || to == id);
                        let (node_text, speaker, used_fallback) = self.story_node_text(node);
                        painter.rect_filled(rect, 8.0, fill);
                        painter.rect_stroke(
                            rect,
                            8.0,
                            egui::Stroke::new(
                                if invalid { 3.0 } else { 1.0 },
                                if invalid {
                                    egui::Color32::from_rgb(235, 150, 120)
                                } else if selected {
                                    egui::Color32::WHITE
                                } else {
                                    egui::Color32::from_rgb(84, 104, 125)
                                },
                            ),
                        );
                        painter.text(
                            rect.left_top() + egui::vec2(10.0, 10.0),
                            egui::Align2::LEFT_TOP,
                            truncate_for_node(&node_text, 30),
                            egui::FontId::proportional(14.0),
                            egui::Color32::WHITE,
                        );
                        painter.text(
                            rect.left_top() + egui::vec2(10.0, 32.0),
                            egui::Align2::LEFT_TOP,
                            truncate_for_node(&format!("{} · {}", speaker, node.node_type), 30),
                            egui::FontId::proportional(12.0),
                            egui::Color32::from_rgb(174, 191, 210),
                        );
                        painter.text(
                            rect.left_top() + egui::vec2(10.0, 54.0),
                            egui::Align2::LEFT_TOP,
                            truncate_for_node(
                                if used_fallback {
                                    "EN fallback"
                                } else {
                                    node.dialogue_id
                                        .as_deref()
                                        .or(node.quest_id.as_deref())
                                        .unwrap_or("No content")
                                },
                                30,
                            ),
                            egui::FontId::proportional(12.0),
                            if used_fallback {
                                egui::Color32::from_rgb(236, 173, 72)
                            } else {
                                egui::Color32::from_rgb(174, 191, 210)
                            },
                        );
                        let response = ui.interact(
                            rect,
                            egui::Id::new(("story_node", story_id, id)),
                            egui::Sense::click_and_drag(),
                        );
                        if response.clicked() {
                            self.selected_node_id = Some(id.clone());
                        }
                        if response.dragged() {
                            if let Some(story) = self.store.content.stories.get_mut(story_id) {
                                if let Some(node) = story.nodes.get_mut(id) {
                                    let delta = response.drag_delta();
                                    node.position[0] += delta.x;
                                    node.position[1] += delta.y;
                                    self.content_dirty = true;
                                }
                            }
                        }
                    }
                });
        }

        fn arrange_story(&mut self, story_id: &str) {
            let Some(story) = self.store.content.stories.get(story_id) else {
                return;
            };
            let (positions, issues) = content::arrange_story_top_down(story);
            if let Some(story) = self.store.content.stories.get_mut(story_id) {
                for (node_id, position) in positions {
                    if let Some(node) = story.nodes.get_mut(&node_id) {
                        node.position = [position.x, position.y];
                    }
                }
            }
            self.content_dirty = true;
            if issues.orphaned_nodes.is_empty() && issues.cycle_edges.is_empty() {
                self.notify("Story arranged top-down.", false);
            } else {
                self.notify(
                    format!(
                        "Story arranged with {} orphaned node(s) and {} cycle edge(s) highlighted.",
                        issues.orphaned_nodes.len(),
                        issues.cycle_edges.len()
                    ),
                    true,
                );
            }
        }

        fn show_story_sequence(&mut self, ui: &mut egui::Ui, story_id: &str) {
            let Some(story) = self.store.content.stories.get(story_id) else {
                return;
            };
            let mut node_ids = Vec::new();
            let mut current = story.entry_node_id.clone();
            let mut seen = BTreeSet::new();
            while seen.insert(current.clone()) {
                node_ids.push(current.clone());
                let Some(node) = story.nodes.get(&current) else {
                    break;
                };
                let Some(next) = node.next.first() else {
                    break;
                };
                current = next.clone();
            }
            for (index, node_id) in node_ids.iter().enumerate() {
                let Some(node) = story.nodes.get(node_id) else {
                    continue;
                };
                if ui
                    .selectable_label(
                        self.selected_node_id.as_deref() == Some(node_id),
                        format!("{:02}  {}  ·  {}", index + 1, node_id, node.node_type),
                    )
                    .clicked()
                {
                    self.selected_node_id = Some(node_id.clone());
                }
                ui.indent(node_id, |ui| {
                    ui.label(
                        node.dialogue_id
                            .as_deref()
                            .or(node.quest_id.as_deref())
                            .unwrap_or("No linked content"),
                    );
                    if let Some(dialogue_id) = &node.dialogue_id {
                        if let Some(dialogue) = self.store.content.dialogues.get(dialogue_id) {
                            for line in &dialogue.lines {
                                if line.kind == "text" {
                                    if let Some(key) = &line.key {
                                        let preview = self.localized_preview(key);
                                        ui.label(format!(
                                            "{}: {}",
                                            line.speaker.as_deref().unwrap_or("Narrator"),
                                            preview.text
                                        ));
                                    }
                                } else if let Some(command) = &line.command {
                                    ui.colored_label(
                                        egui::Color32::from_rgb(236, 173, 72),
                                        format!("Command: {command}"),
                                    );
                                }
                            }
                        }
                    }
                    ui.label(format!(
                        "Next: {}",
                        if node.next.is_empty() {
                            "end".into()
                        } else {
                            node.next.join(", ")
                        }
                    ));
                });
            }
        }

        fn start_reader_for_selected_story(&mut self) {
            let Some(story_id) = self.selected_story_id.clone() else {
                return;
            };
            self.reader.locale = self.locale.clone();
            if self.reader.story_id.as_deref() != Some(story_id.as_str()) {
                self.reader.story_id = Some(story_id.clone());
                self.reader.current_node_id = self
                    .store
                    .content
                    .stories
                    .get(&story_id)
                    .map(|story| story.entry_node_id.clone());
                self.reader.history.clear();
                self.reader.selected_choice = None;
                self.reader.active = true;
            } else if self.reader.current_node_id.is_none() {
                self.reader.current_node_id = self
                    .store
                    .content
                    .stories
                    .get(&story_id)
                    .map(|story| story.entry_node_id.clone());
                self.reader.active = true;
            }
        }

        fn reader_restart(&mut self) {
            let Some(story_id) = self.reader.story_id.clone() else {
                return;
            };
            let entry_node_id = self
                .store
                .content
                .stories
                .get(&story_id)
                .map(|story| story.entry_node_id.clone());
            if let Some(entry_node_id) = entry_node_id {
                self.reader
                    .restart_at(story_id, entry_node_id, self.locale.clone());
            }
        }

        fn reader_back(&mut self) {
            self.reader.back();
        }

        fn reader_advance_to(&mut self, next: String) {
            self.reader.advance_to(next);
        }

        pub(crate) fn localized_preview(&self, key: &str) -> LocalizedPreview {
            let source = self.store.source.entries.get(key);
            let source_text = source
                .map(|entry| entry.text.clone())
                .unwrap_or_else(|| format!("Missing key: {key}"));
            if self.locale == self.store.catalog.source_locale {
                return LocalizedPreview {
                    text: source_text,
                    used_fallback: false,
                    status: "source".into(),
                };
            }
            let current_source_hash = source.map(|entry| hash_text(&entry.text));
            if let Some(entry) = self
                .store
                .targets
                .get(&self.locale)
                .and_then(|file| file.entries.get(key))
            {
                if entry.status == ACCEPTED
                    && Some(entry.source_hash.clone()) == current_source_hash
                {
                    return LocalizedPreview {
                        text: entry.text.clone(),
                        used_fallback: false,
                        status: ACCEPTED.into(),
                    };
                }
                return LocalizedPreview {
                    text: source_text,
                    used_fallback: true,
                    status: entry.status.clone(),
                };
            }
            LocalizedPreview {
                text: source_text,
                used_fallback: true,
                status: "missing".into(),
            }
        }

        fn story_node_text(&self, node: &content::StoryNode) -> (String, String, bool) {
            if let Some(dialogue_id) = &node.dialogue_id {
                if let Some(dialogue) = self.store.content.dialogues.get(dialogue_id) {
                    for line in &dialogue.lines {
                        if line.kind != "text" {
                            continue;
                        }
                        if let Some(key) = &line.key {
                            let preview = self.localized_preview(key);
                            return (
                                preview
                                    .text
                                    .replace('\n', " ")
                                    .split_whitespace()
                                    .collect::<Vec<_>>()
                                    .join(" "),
                                line.speaker.clone().unwrap_or_else(|| "Narrator".into()),
                                preview.used_fallback,
                            );
                        }
                    }
                    if !dialogue.name_key.is_empty() {
                        let preview = self.localized_preview(&dialogue.name_key);
                        return (preview.text, "Dialogue".into(), preview.used_fallback);
                    }
                }
                return (dialogue_id.clone(), "Dialogue".into(), false);
            }
            if !node.name_key.is_empty() {
                let preview = self.localized_preview(&node.name_key);
                return (preview.text, node.node_type.clone(), preview.used_fallback);
            }
            if let Some(quest_id) = &node.quest_id {
                if let Some(quest) = self.store.content.quests.get(quest_id) {
                    if !quest.name_key.is_empty() {
                        let preview = self.localized_preview(&quest.name_key);
                        return (preview.text, "Quest".into(), preview.used_fallback);
                    }
                }
                return (quest_id.clone(), "Quest".into(), false);
            }
            if let Some(action) = &node.action {
                return (action.type_name().into(), "Action".into(), false);
            }
            if let Some(condition) = &node.condition {
                return (condition.condition_type.clone(), "Condition".into(), false);
            }
            (node.node_type.clone(), "Node".into(), false)
        }

        fn show_story_reader(&mut self, ui: &mut egui::Ui, story_id: &str) {
            self.start_reader_for_selected_story();
            self.reader.locale = self.locale.clone();
            let Some(current_node_id) = self.reader.current_node_id.clone() else {
                ui.centered_and_justified(|ui| ui.heading("Select a story to start reading"));
                return;
            };
            let Some(story) = self.store.content.stories.get(story_id) else {
                return;
            };
            let Some(node) = story.nodes.get(&current_node_id).cloned() else {
                ui.colored_label(
                    egui::Color32::from_rgb(235, 150, 120),
                    "Reader stopped: current node no longer exists.",
                );
                return;
            };
            let chapter = node.chapter_id.as_deref().unwrap_or("main");
            let story_name = if story.name_key.is_empty() {
                story_id.to_string()
            } else {
                self.localized_preview(&story.name_key).text
            };
            ui.horizontal(|ui| {
                ui.heading(format!("{story_name} · Story reader"));
                ui.separator();
                ui.label(format!("Chapter: {chapter}"));
                ui.label(format!("Node: {current_node_id}"));
                if self.reader.history.is_empty() {
                    ui.colored_label(egui::Color32::from_rgb(136, 215, 164), "At start");
                }
            });
            ui.label(
                "Preview only. Actions and callbacks are simulated and never mutate a game save.",
            );
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(!self.reader.history.is_empty(), egui::Button::new("Back"))
                    .clicked()
                {
                    self.reader_back();
                }
                if ui.button("Restart").clicked() {
                    self.reader_restart();
                }
                ui.label(format!("Locale: {}", self.locale));
            });

            let mut transition: Option<String> = None;
            egui::ScrollArea::vertical()
                .id_salt("story_reader_scroll")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.group(|ui| {
                        ui.label(format!("{} node", node.node_type));
                        if let Some(dialogue_id) = &node.dialogue_id {
                            if let Some(dialogue) = self.store.content.dialogues.get(dialogue_id) {
                                for line in &dialogue.lines {
                                    if line.kind == "text" {
                                        if let Some(key) = &line.key {
                                            let preview = self.localized_preview(key);
                                            ui.horizontal_wrapped(|ui| {
                                                ui.strong(format!(
                                                    "{}:",
                                                    line.speaker.as_deref().unwrap_or("Narrator")
                                                ));
                                                ui.label(&preview.text);
                                            });
                                            if preview.used_fallback {
                                                ui.colored_label(
                                                    egui::Color32::from_rgb(236, 173, 72),
                                                    format!(
                                                        "Fallback to English ({})",
                                                        status_display(&preview.status)
                                                    ),
                                                );
                                            }
                                        }
                                    } else if let Some(command) = &line.command {
                                        ui.colored_label(
                                            egui::Color32::from_rgb(236, 173, 72),
                                            format!("Non-translatable command · {command}"),
                                        );
                                    }
                                }
                            } else {
                                ui.colored_label(
                                    egui::Color32::from_rgb(235, 150, 120),
                                    "Missing dialogue definition",
                                );
                            }
                        }
                        if let Some(condition) = &node.condition {
                            ui.colored_label(
                                egui::Color32::from_rgb(196, 158, 46),
                                format!("Condition simulated · {}", condition.condition_type),
                            );
                        }
                        if let Some(action) = &node.action {
                            ui.colored_label(
                                egui::Color32::from_rgb(208, 116, 45),
                                format!("Action simulated · {}", action.type_name()),
                            );
                            if matches!(action, content::ActionDefinition::Callback { .. }) {
                                ui.colored_label(
                                    egui::Color32::from_rgb(235, 150, 120),
                                    "Java callback required",
                                );
                            }
                        }
                        if let Some(quest_id) = &node.quest_id {
                            ui.label(format!("Quest affected · {quest_id}"));
                        }
                        if node.node_type == "end" {
                            ui.colored_label(
                                egui::Color32::from_rgb(136, 215, 164),
                                "End of story",
                            );
                        }
                    });

                    if !node.choices.is_empty() {
                        ui.separator();
                        ui.strong("Choose a branch");
                        for choice in &node.choices {
                            let label = if choice.label_key.is_empty() {
                                choice.id.clone()
                            } else {
                                self.localized_preview(&choice.label_key).text
                            };
                            if ui.button(label).clicked() {
                                transition = Some(choice.next.clone());
                                self.reader.selected_choice = Some(choice.id.clone());
                            }
                        }
                    } else if let Some(next) = node.next.first() {
                        if ui.button("Next").clicked() {
                            transition = Some(next.clone());
                        }
                    } else if node.node_type != "end" {
                        ui.colored_label(
                            egui::Color32::from_rgb(235, 150, 120),
                            "No outgoing connection",
                        );
                    }
                });
            if let Some(next) = transition {
                self.reader_advance_to(next);
            }
        }

        fn show_story_inspector(&mut self, context: &egui::Context) {
            let story_id = self.selected_story_id.clone();
            let node_id = self.selected_node_id.clone();
            let mut delete_node = false;
            egui::SidePanel::right("story_inspector")
            .default_width(INSPECTOR_WIDTH)
            .width_range(SIDE_PANEL_MIN_WIDTH..=SIDE_PANEL_MAX_WIDTH)
            .resizable(true)
            .frame(panel_frame(panel_background(), 12.0))
            .show(context, |ui| {
                constrain_side_panel_content(ui);
                ui.heading("Inspector");
                egui::ScrollArea::vertical()
                    .max_width(ui.available_width())
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                    let Some(story_id) = story_id.as_deref() else { ui.label("Select a story."); return; };
                    if let Some(node_id) = node_id.as_deref() {
                        ui.label(format!("Node ID: {node_id}"));
                        if let Some(story) = self.store.content.stories.get_mut(story_id) {
                            let options: Vec<String> = story
                                .nodes
                                .keys()
                                .filter(|candidate| *candidate != node_id)
                                .cloned()
                                .collect();
                            if let Some(node) = story.nodes.get_mut(node_id) {
                                ui.horizontal(|ui| { ui.label("Type"); egui::ComboBox::from_id_salt("node_type").selected_text(&node.node_type).show_ui(ui, |ui| { for value in ["dialogue","choice","condition","action","quest_start","quest_update","scene_transition","event","end"] { ui.selectable_value(&mut node.node_type, value.into(), value); } }); });
                                ui.label("Display name key");
                                if ui.text_edit_singleline(&mut node.name_key).changed() { self.content_dirty = true; }
                                if let Some(value) = &mut node.dialogue_id {
                                    ui.label("Dialogue ID"); if ui.text_edit_singleline(value).changed() { self.content_dirty = true; }
                                } else if ui.button("Link dialogue").clicked() { node.dialogue_id = Some(String::new()); self.content_dirty = true; }
                                if let Some(value) = &mut node.quest_id {
                                    ui.label("Quest ID"); if ui.text_edit_singleline(value).changed() { self.content_dirty = true; }
                                } else if ui.button("Link quest").clicked() { node.quest_id = Some(String::new()); self.content_dirty = true; }
                                ui.separator();
                                ui.label("Next nodes");
                                for next in &node.next { ui.label(format!("→ {next}")); }
                                egui::ComboBox::from_id_salt("add_next_node").selected_text("Add connection...").show_ui(ui, |ui| { for target in options { if ui.button(&target).clicked() { if !node.next.contains(&target) { node.next.push(target); self.content_dirty = true; } ui.close_menu(); } } });
                                if !node.choices.is_empty() { ui.label(format!("{} choice outputs", node.choices.len())); }
                                if node.action.is_some() { ui.label("Has declarative action"); }
                                if node.condition.is_some() { ui.label("Has condition"); }
                                if node_id != story.entry_node_id && ui.button("Delete node").clicked() { delete_node = true; }
                            }
                        }
                    } else if let Some(story) = self.store.content.stories.get_mut(story_id) {
                        ui.label(format!("Story ID: {story_id}"));
                        ui.label("Name key"); if ui.text_edit_singleline(&mut story.name_key).changed() { self.content_dirty = true; }
                        ui.label("Description key"); if ui.text_edit_singleline(&mut story.description_key).changed() { self.content_dirty = true; }
                        ui.label("Owner ID"); if ui.text_edit_singleline(&mut story.owner_id).changed() { self.content_dirty = true; }
                        ui.label(format!("Entry node: {}", story.entry_node_id));
                        ui.label(format!("{} chapters", story.chapters.len()));
                        ui.colored_label(egui::Color32::from_rgb(174, 191, 210), "IDs are stable technical identifiers. Edit name keys instead of renaming IDs.");
                    }
                });
            });
            if delete_node {
                if let (Some(story_id), Some(node_id)) = (story_id, node_id) {
                    if let Some(story) = self.store.content.stories.get_mut(&story_id) {
                        story.nodes.remove(&node_id);
                        for other in story.nodes.values_mut() {
                            other.next.retain(|next| next != &node_id);
                            other.choices.retain(|choice| choice.next != node_id);
                        }
                        for chapter in &mut story.chapters {
                            chapter.node_ids.retain(|id| id != &node_id);
                        }
                    }
                    self.selected_node_id = None;
                    self.content_dirty = true;
                }
            }
        }

        fn show_catalog_section(&mut self, context: &egui::Context) {
            let section = self.content_section;
            egui::SidePanel::left("catalog_explorer")
                .default_width(EXPLORER_WIDTH)
                .width_range(SIDE_PANEL_MIN_WIDTH..=SIDE_PANEL_MAX_WIDTH)
                .resizable(true)
                .frame(panel_frame(panel_background(), 12.0))
                .show(context, |ui| {
                    constrain_side_panel_content(ui);
                    ui.heading(match section {
                        ContentSection::Quests => "Quests",
                        ContentSection::Dialogues => "Dialogues",
                        ContentSection::Items => "Items",
                        ContentSection::Entities => "Entities",
                        ContentSection::Stories => "Stories",
                    });
                    ui.label("Select an entry to edit its stable ID and references.");
                    let new_label = match section {
                        ContentSection::Quests => "New quest",
                        ContentSection::Dialogues => "New dialogue",
                        ContentSection::Items => "New item",
                        ContentSection::Entities => "New entity",
                        ContentSection::Stories => "New story",
                    };
                    if ui.button(new_label).clicked() {
                        let id = match section {
                            ContentSection::Quests => {
                                next_content_id("quest", self.store.content.quests.keys())
                            }
                            ContentSection::Dialogues => {
                                next_content_id("dialogue", self.store.content.dialogues.keys())
                            }
                            ContentSection::Items => {
                                next_content_id("item", self.store.content.items.keys())
                            }
                            ContentSection::Entities => {
                                next_content_id("entity", self.store.content.entities.keys())
                            }
                            ContentSection::Stories => return,
                        };
                        match section {
                            ContentSection::Quests => {
                                self.store.content.quests.insert(
                                    id.clone(),
                                    content::QuestDefinition {
                                        name_key: String::new(),
                                        description_keys: Vec::new(),
                                        story_id: None,
                                        chapter_id: None,
                                        owner_id: None,
                                        stages: Vec::new(),
                                        requirements: Vec::new(),
                                        start_actions: Vec::new(),
                                        complete_actions: Vec::new(),
                                        fail_actions: Vec::new(),
                                        initial_status: "PENDING".into(),
                                    },
                                );
                            }
                            ContentSection::Dialogues => {
                                self.store.content.dialogues.insert(
                                    id.clone(),
                                    content::DialogueDefinition {
                                        name_key: String::new(),
                                        story_id: None,
                                        chapter_id: None,
                                        owner_id: None,
                                        lines: Vec::new(),
                                        on_enter: Vec::new(),
                                        on_exit: Vec::new(),
                                    },
                                );
                            }
                            ContentSection::Items => {
                                self.store.content.items.insert(
                                    id.clone(),
                                    content::ItemDefinition {
                                        name_key: String::new(),
                                        description_key: String::new(),
                                        category: "misc".into(),
                                        sprite_ref: String::new(),
                                        stackable: true,
                                        max_stack: Some(99),
                                        effects: Vec::new(),
                                        tags: Vec::new(),
                                        quest_item: false,
                                    },
                                );
                            }
                            ContentSection::Entities => {
                                self.store.content.entities.insert(
                                    id.clone(),
                                    content::EntityDefinition {
                                        name_key: String::new(),
                                        entity_type: "npc".into(),
                                        sprite_ref: String::new(),
                                        owner_id: String::new(),
                                        dialogue_ids: Vec::new(),
                                        interaction_conditions: Vec::new(),
                                        interaction_actions: Vec::new(),
                                        states: vec!["default".into()],
                                        initial_state: "default".into(),
                                    },
                                );
                            }
                            ContentSection::Stories => {}
                        }
                        self.selected_content_id = Some(id);
                        self.content_dirty = true;
                        self.notify(format!("Created {}.", new_label), false);
                    }
                    ui.separator();
                    egui::ScrollArea::vertical()
                        .max_width(ui.available_width())
                        .show(ui, |ui| match section {
                            ContentSection::Quests => {
                                for id in self.store.content.quests.keys() {
                                    if ui
                                        .add_sized(
                                            [ui.available_width(), 30.0],
                                            egui::SelectableLabel::new(
                                                self.selected_content_id.as_deref()
                                                    == Some(id.as_str()),
                                                id,
                                            ),
                                        )
                                        .clicked()
                                    {
                                        self.selected_content_id = Some(id.clone());
                                    }
                                }
                            }
                            ContentSection::Dialogues => {
                                for id in self.store.content.dialogues.keys() {
                                    if ui
                                        .add_sized(
                                            [ui.available_width(), 30.0],
                                            egui::SelectableLabel::new(
                                                self.selected_content_id.as_deref()
                                                    == Some(id.as_str()),
                                                id,
                                            ),
                                        )
                                        .clicked()
                                    {
                                        self.selected_content_id = Some(id.clone());
                                    }
                                }
                            }
                            ContentSection::Items => {
                                for id in self.store.content.items.keys() {
                                    if ui
                                        .add_sized(
                                            [ui.available_width(), 30.0],
                                            egui::SelectableLabel::new(
                                                self.selected_content_id.as_deref()
                                                    == Some(id.as_str()),
                                                id,
                                            ),
                                        )
                                        .clicked()
                                    {
                                        self.selected_content_id = Some(id.clone());
                                    }
                                }
                            }
                            ContentSection::Entities => {
                                for id in self.store.content.entities.keys() {
                                    if ui
                                        .add_sized(
                                            [ui.available_width(), 30.0],
                                            egui::SelectableLabel::new(
                                                self.selected_content_id.as_deref()
                                                    == Some(id.as_str()),
                                                id,
                                            ),
                                        )
                                        .clicked()
                                    {
                                        self.selected_content_id = Some(id.clone());
                                    }
                                }
                            }
                            ContentSection::Stories => {}
                        });
                });
            egui::CentralPanel::default().frame(panel_frame(app_background(), 16.0)).show(context, |ui| {
            let Some(id) = self.selected_content_id.clone() else {
                ui.centered_and_justified(|ui| ui.heading("Create or select an entry"));
                return;
            };
            ui.heading(format!("{} · {}", id, match section { ContentSection::Quests => "Quest", ContentSection::Dialogues => "Dialogue", ContentSection::Items => "Item", ContentSection::Entities => "Entity", ContentSection::Stories => "Story" }));
            ui.label("Stable IDs are read-only. Names, references and gameplay metadata are editable.");
            egui::ScrollArea::vertical().show(ui, |ui| match section {
                ContentSection::Quests => if let Some(value) = self.store.content.quests.get_mut(&id) {
                    edit_text(ui, "Name key", &mut value.name_key, &mut self.content_dirty);
                    edit_text(ui, "Owner ID", value.owner_id.get_or_insert_with(String::new), &mut self.content_dirty);
                    edit_text(ui, "Story ID", value.story_id.get_or_insert_with(String::new), &mut self.content_dirty);
                    edit_text(ui, "Initial status", &mut value.initial_status, &mut self.content_dirty);
                    ui.separator(); ui.strong("Requirements");
                    for requirement in &value.requirements { ui.label(format!("{} · {}", requirement.requirement_type, requirement.item_id.as_deref().or(requirement.entity_id.as_deref()).or(requirement.callback_id.as_deref()).unwrap_or("custom"))); }
                },
                ContentSection::Dialogues => if let Some(value) = self.store.content.dialogues.get_mut(&id) {
                    edit_text(ui, "Name key", &mut value.name_key, &mut self.content_dirty);
                    edit_text(ui, "Owner ID", value.owner_id.get_or_insert_with(String::new), &mut self.content_dirty);
                    edit_text(ui, "Story ID", value.story_id.get_or_insert_with(String::new), &mut self.content_dirty);
                    ui.separator(); ui.strong(format!("{} lines", value.lines.len()));
                    for line in &value.lines { ui.label(format!("{} · {} · {}", line.id, line.speaker.as_deref().unwrap_or("Narrator"), line.key.as_deref().or(line.command.as_deref()).unwrap_or(""))); }
                },
                ContentSection::Items => if let Some(value) = self.store.content.items.get_mut(&id) {
                    edit_text(ui, "Name key", &mut value.name_key, &mut self.content_dirty); edit_text(ui, "Description key", &mut value.description_key, &mut self.content_dirty); edit_text(ui, "Category", &mut value.category, &mut self.content_dirty); edit_text(ui, "Existing sprite ref", &mut value.sprite_ref, &mut self.content_dirty); ui.checkbox(&mut value.stackable, "Stackable"); ui.checkbox(&mut value.quest_item, "Quest item");
                },
                ContentSection::Entities => if let Some(value) = self.store.content.entities.get_mut(&id) {
                    edit_text(ui, "Name key", &mut value.name_key, &mut self.content_dirty); edit_text(ui, "Entity type", &mut value.entity_type, &mut self.content_dirty); edit_text(ui, "Registered sprite ref", &mut value.sprite_ref, &mut self.content_dirty); edit_text(ui, "Owner ID", &mut value.owner_id, &mut self.content_dirty); edit_text(ui, "Initial state", &mut value.initial_state, &mut self.content_dirty); ui.separator(); ui.label(format!("Dialogues: {}", value.dialogue_ids.join(", "))); ui.label(format!("States: {}", value.states.join(", ")));
                },
                ContentSection::Stories => {}
            });
        });
        }

        fn render_notification(&mut self, context: &egui::Context) {
            if let Some(notification) = &self.notification {
                if notification.expires_at > Instant::now() {
                    egui::TopBottomPanel::bottom("studio_notification")
                        .frame(panel_frame(toolbar_background(), 8.0))
                        .show(context, |ui| {
                            let color = if notification.is_error {
                                egui::Color32::from_rgb(235, 150, 120)
                            } else {
                                egui::Color32::from_rgb(133, 210, 157)
                            };
                            ui.colored_label(color, &notification.message);
                        });
                } else {
                    self.notification = None;
                }
            }
        }
    }

    impl eframe::App for EditorApp {
        fn update(&mut self, context: &egui::Context, _frame: &mut eframe::Frame) {
            configure_theme(context);
            self.check_external_changes();

            let mut pending_action = None;
            let mut pending_context_action = None;
            let mut locale_changed = false;
            let mut clear_filters = false;
            let mut chip_to_clear = None;
            let previous_locale = self.locale.clone();

            let types = self.values_for("type");
            let stories = self.values_for("story");
            let quests = self.values_for("quest");
            let owners = self.values_for("owner");
            egui::TopBottomPanel::top("app_toolbar")
                .frame(panel_frame(toolbar_background(), 12.0))
                .show(context, |ui| {
                    // Keep navigation and actions on deterministic rows. Wrapping a
                    // toolbar mid-group makes controls jump to different baselines
                    // on small windows and makes the primary actions hard to scan.
                    toolbar_row(ui, 34.0, |ui| {
                        ui.add_sized(
                            [178.0, 34.0],
                            egui::Label::new(egui::RichText::new("KStory").size(20.0).strong()),
                        );
                        ui.separator();
                        if ui
                            .add_sized(
                                [144.0, 32.0],
                                egui::SelectableLabel::new(self.show_content, "Content explorer"),
                            )
                            .clicked()
                        {
                            self.show_content = true;
                            self.show_studio = false;
                        }
                        if ui
                            .add_sized(
                                [132.0, 32.0],
                                egui::SelectableLabel::new(self.show_studio, "Content studio"),
                            )
                            .clicked()
                        {
                            self.show_studio = true;
                            self.show_content = false;
                        }
                        if ui
                            .add_sized(
                                [124.0, 32.0],
                                egui::SelectableLabel::new(
                                    !self.show_content && !self.show_studio,
                                    "Key registry",
                                ),
                            )
                            .clicked()
                        {
                            self.show_content = false;
                            self.show_studio = false;
                        }
                    });
                    ui.add_space(4.0);
                    toolbar_row(ui, 32.0, |ui| {
                        ui.add_sized(
                            [48.0, 30.0],
                            egui::Label::new(egui::RichText::new("Locale").strong()),
                        );
                        egui::ComboBox::from_id_salt("locale_selector")
                            .selected_text(locale_name(&self.locale))
                            .width(108.0)
                            .show_ui(ui, |ui| {
                                for locale in
                                    std::iter::once(self.store.catalog.source_locale.clone())
                                        .chain(self.store.catalog.target_locales.iter().cloned())
                                {
                                    if ui
                                        .selectable_value(
                                            &mut self.locale,
                                            locale.clone(),
                                            locale_name(&locale),
                                        )
                                        .changed()
                                    {
                                        locale_changed = true;
                                        ui.memory_mut(|memory| memory.close_popup());
                                    }
                                }
                            });
                        if ui
                            .add_sized([70.0, 32.0], egui::Button::new("Sync"))
                            .clicked()
                        {
                            match self.store.sync() {
                                Ok(report) => self.notify(
                                    format!(
                                        "Synced: {} new keys, {} source changes, {} orphaned keys.",
                                        report.created_keys,
                                        report.changed_sources,
                                        report.orphaned_keys
                                    ),
                                    false,
                                ),
                                Err(error) => self.notify(error.to_string(), true),
                            }
                        }
                        if ui
                            .add_sized([76.0, 32.0], egui::Button::new("Export"))
                            .clicked()
                        {
                            match self.store.export() {
                                Ok(()) => self.notify("Runtime bundles exported.", false),
                                Err(error) => self.notify(error.to_string(), true),
                            }
                        }
                        let inspector_label = if self.show_inspector {
                            "Hide inspector"
                        } else {
                            "Show inspector"
                        };
                        if ui
                            .add_sized([116.0, 32.0], egui::Button::new(inspector_label))
                            .clicked()
                        {
                            self.show_inspector = !self.show_inspector;
                        }
                    });
                    ui.separator();
                    toolbar_row(ui, 24.0, |ui| {
                        ui.label(egui::RichText::new("Translation status").strong());
                        let counts = self.store.status_counts(&self.locale);
                        if self.locale == self.store.catalog.source_locale {
                            ui.colored_label(
                                status_color("source"),
                                format!("source: {}", counts.get("source").copied().unwrap_or(0)),
                            );
                        } else {
                            for status in [WAITING, ON_REVIEW, ACCEPTED] {
                                ui.colored_label(
                                    status_color(status),
                                    format!(
                                        "{}: {}",
                                        status.replace('_', " "),
                                        counts.get(status).copied().unwrap_or(0)
                                    ),
                                );
                            }
                        }
                    });
                });

            if self.show_studio {
                self.show_content_studio(context);
                self.render_notification(context);
                context.request_repaint_after(Duration::from_millis(250));
                return;
            }

            egui::TopBottomPanel::top("filter_toolbar")
                .frame(panel_frame(panel_background(), 10.0))
                .show(context, |ui| {
                    // Keep every row inside an explicit width. The compact variant
                    // uses separate control rows so no label or combo can be
                    // pushed outside the left or right edge of the window.
                    let compact_filters = ui.available_width() < 1000.0;
                    let status_options: Vec<&str> =
                        if self.locale == self.store.catalog.source_locale {
                            vec!["source"]
                        } else {
                            vec![WAITING, ON_REVIEW, ACCEPTED]
                        };

                    if compact_filters {
                        toolbar_row(ui, 30.0, |ui| {
                            ui.add_sized(
                                [54.0, 30.0],
                                egui::Label::new(egui::RichText::new("Search").strong()),
                            );
                            let search_width = (ui.available_width() - 2.0).max(150.0);
                            ui.add_sized(
                                [search_width, 30.0],
                                egui::TextEdit::singleline(&mut self.filters.search)
                                    .id_salt("global_search")
                                    .hint_text(
                                        "key, source, translation, story, quest, owner or speaker",
                                    ),
                            );
                        });
                        ui.add_space(6.0);
                        toolbar_row(ui, 30.0, |ui| {
                            let control_width = ((ui.available_width() - 16.0) / 3.0).max(120.0);
                            filter_combo_sized(
                                ui,
                                "Status",
                                &mut self.filters.status,
                                status_options,
                                control_width,
                            );
                            filter_combo_sized(
                                ui,
                                "Type",
                                &mut self.filters.entry_type,
                                types.iter().map(String::as_str),
                                control_width,
                            );
                            sort_combo(ui, &mut self.filters.sort_mode, control_width);
                        });
                        ui.add_space(6.0);
                        toolbar_row(ui, 30.0, |ui| {
                            let control_width = ((ui.available_width() - 16.0) / 3.0).max(120.0);
                            filter_combo_sized(
                                ui,
                                "Story",
                                &mut self.filters.story,
                                stories.iter().map(String::as_str),
                                control_width,
                            );
                            filter_combo_sized(
                                ui,
                                "Quest",
                                &mut self.filters.quest,
                                quests.iter().map(String::as_str),
                                control_width,
                            );
                            filter_combo_sized(
                                ui,
                                "Owner",
                                &mut self.filters.owner,
                                owners.iter().map(String::as_str),
                                control_width,
                            );
                        });
                        ui.add_space(6.0);
                        toolbar_row(ui, 30.0, |ui| {
                            ui.checkbox(&mut self.filters.only_orphaned, "Orphaned only");
                            if ui
                                .add_sized([112.0, 30.0], egui::Button::new("Clear filters"))
                                .clicked()
                            {
                                clear_filters = true;
                            }
                        });
                    } else {
                        toolbar_row(ui, 30.0, |ui| {
                            ui.add_sized(
                                [54.0, 30.0],
                                egui::Label::new(egui::RichText::new("Search").strong()),
                            );
                            let control_width = 132.0;
                            let search_width = (ui.available_width()
                                - 3.0 * control_width
                                - 3.0 * ui.spacing().item_spacing.x
                                - 2.0)
                                .max(240.0);
                            ui.add_sized(
                                [search_width, 30.0],
                                egui::TextEdit::singleline(&mut self.filters.search)
                                    .id_salt("global_search")
                                    .hint_text(
                                        "key, source, translation, story, quest, owner or speaker",
                                    ),
                            );
                            filter_combo_sized(
                                ui,
                                "Status",
                                &mut self.filters.status,
                                status_options,
                                control_width,
                            );
                            filter_combo_sized(
                                ui,
                                "Type",
                                &mut self.filters.entry_type,
                                types.iter().map(String::as_str),
                                control_width,
                            );
                            sort_combo(ui, &mut self.filters.sort_mode, control_width);
                        });
                        ui.add_space(6.0);
                        toolbar_row(ui, 30.0, |ui| {
                            filter_combo_sized(
                                ui,
                                "Story",
                                &mut self.filters.story,
                                stories.iter().map(String::as_str),
                                150.0,
                            );
                            filter_combo_sized(
                                ui,
                                "Quest",
                                &mut self.filters.quest,
                                quests.iter().map(String::as_str),
                                150.0,
                            );
                            filter_combo_sized(
                                ui,
                                "Owner",
                                &mut self.filters.owner,
                                owners.iter().map(String::as_str),
                                150.0,
                            );
                            ui.checkbox(&mut self.filters.only_orphaned, "Orphaned only");
                            if ui
                                .add_sized([112.0, 30.0], egui::Button::new("Clear filters"))
                                .clicked()
                            {
                                clear_filters = true;
                            }
                        });
                    }
                    if self.filters.is_active() {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(egui::RichText::new("Active filters").strong());
                            if !self.filters.search.is_empty()
                                && ui
                                    .small_button(format!("Search: {} ×", self.filters.search))
                                    .clicked()
                            {
                                chip_to_clear = Some("Search");
                            }
                            for (label, value) in [
                                ("Status", &self.filters.status),
                                ("Type", &self.filters.entry_type),
                                ("Story", &self.filters.story),
                                ("Quest", &self.filters.quest),
                                ("Owner", &self.filters.owner),
                            ] {
                                if !value.is_empty()
                                    && ui.small_button(format!("{label}: {value} ×")).clicked()
                                {
                                    chip_to_clear = Some(label);
                                }
                            }
                        });
                    }
                });

            if clear_filters {
                self.filters.clear();
            }
            if locale_changed {
                if self.draft_dirty {
                    self.locale = previous_locale;
                    self.notify(
                        "Save or reset the current draft before changing locale.",
                        true,
                    );
                } else {
                    if self.locale == self.store.catalog.source_locale {
                        if !self.filters.status.is_empty() && self.filters.status != "source" {
                            self.filters.status.clear();
                        }
                    } else if self.filters.status == "source" {
                        self.filters.status.clear();
                    }
                    self.load_draft();
                }
            }
            if let Some(label) = chip_to_clear {
                match label {
                    "Status" => self.filters.status.clear(),
                    "Type" => self.filters.entry_type.clear(),
                    "Story" => self.filters.story.clear(),
                    "Quest" => self.filters.quest.clear(),
                    "Owner" => self.filters.owner.clear(),
                    "Search" => self.filters.search.clear(),
                    _ => {}
                }
            }

            let mut visible_keys = self.store.filtered_keys(&self.locale, &self.filters);
            if self.show_content {
                visible_keys.retain(|key| {
                    self.store
                        .catalog
                        .entries
                        .get(key)
                        .map(|metadata| {
                            metadata.entry_type == "dialogue_line"
                                || metadata.entry_type == "quest_description"
                        })
                        .unwrap_or(false)
                });
            }
            self.keyboard_action(context, &visible_keys);
            egui::SidePanel::left("explorer")
                .default_width(EXPLORER_WIDTH)
                .width_range(SIDE_PANEL_MIN_WIDTH..=SIDE_PANEL_MAX_WIDTH)
                .resizable(true)
                .frame(panel_frame(panel_background(), 12.0))
                .show(context, |ui| {
                    constrain_side_panel_content(ui);
                    ui.horizontal(|ui| {
                        ui.heading(if self.show_content {
                            "Content explorer"
                        } else {
                            "Key registry"
                        });
                        ui.add_space(4.0);
                        ui.weak(format!("{} results", visible_keys.len()));
                    });
                    ui.separator();
                    egui::ScrollArea::vertical()
                        .id_salt("explorer_scroll")
                        .max_width(ui.available_width())
                        .scroll_bar_visibility(
                            egui::containers::scroll_area::ScrollBarVisibility::VisibleWhenNeeded,
                        )
                        .auto_shrink([false, false])
                        .show_rows(ui, 54.0, visible_keys.len(), |ui, row_range| {
                            for index in row_range {
                                let key = &visible_keys[index];
                                let Some(metadata) = self.store.catalog.entries.get(key) else {
                                    continue;
                                };
                                let story = metadata.story_id.as_deref().unwrap_or("Unassigned");
                                let status = self.target_status(key).to_string();
                                let status_text = status_display(&status);
                                let can_edit_locale =
                                    self.locale != self.store.catalog.source_locale;
                                let row_valid =
                                    self.store.validate_entry(&self.locale, key).is_empty();
                                let selected = self.selected.as_deref() == Some(key.as_str());
                                let row_safe = !selected || !self.draft_dirty;
                                let (row_rect, _) = ui.allocate_exact_size(
                                    egui::vec2(ui.available_width(), 54.0),
                                    egui::Sense::click(),
                                );
                                if selected || ui.rect_contains_pointer(row_rect) {
                                    let fill = if selected {
                                        egui::Color32::from_rgb(47, 86, 126)
                                    } else {
                                        egui::Color32::from_rgb(38, 47, 59)
                                    };
                                    ui.painter().rect_filled(row_rect, 5.0, fill);
                                }
                                ui.scope_builder(
                                    egui::UiBuilder::new().max_rect(row_rect.shrink(8.0)),
                                    |ui| {
                                        ui.with_layout(
                                            egui::Layout::left_to_right(egui::Align::Center),
                                            |ui| {
                                                ui.add_sized(
                                                    [18.0, 36.0],
                                                    egui::Label::new(
                                                        egui::RichText::new("●")
                                                            .color(status_color(&status)),
                                                    ),
                                                );
                                                ui.vertical(|ui| {
                                                    ui.add(
                                                        egui::Label::new(
                                                            egui::RichText::new(key).strong(),
                                                        )
                                                        .truncate(),
                                                    )
                                                    .on_hover_text(key);
                                                    ui.add(
                                                        egui::Label::new(format!(
                                                            "{story} · {} · {status_text}",
                                                            metadata.entry_type
                                                        ))
                                                        .truncate(),
                                                    );
                                                });
                                            },
                                        );
                                    },
                                );
                                let response = ui.interact(
                                    row_rect,
                                    egui::Id::new(("entry_row", key.as_str())),
                                    egui::Sense::click(),
                                );
                                if response.clicked() {
                                    self.request_selection(key.clone());
                                }
                                response.context_menu(|ui| {
                                    ui.label(key);
                                    ui.separator();
                                    if ui.button("Open").clicked() {
                                        self.request_selection(key.clone());
                                        ui.close_menu();
                                    }
                                    if ui.button("Copy key").clicked() {
                                        ui.ctx().copy_text(key.clone());
                                        ui.close_menu();
                                    }
                                    if ui.button("Copy source text").clicked() {
                                        if let Some(source) = self.store.source.entries.get(key) {
                                            ui.ctx().copy_text(source.text.clone());
                                        }
                                        ui.close_menu();
                                    }
                                    if ui.button("Copy translation").clicked() {
                                        if let Some(view) = self.store.entry_view(&self.locale, key)
                                        {
                                            ui.ctx().copy_text(view.target_text);
                                        }
                                        ui.close_menu();
                                    }
                                    ui.separator();
                                    if ui
                                        .add_enabled(
                                            can_edit_locale && status != WAITING && row_safe,
                                            egui::Button::new("Mark as waiting"),
                                        )
                                        .clicked()
                                    {
                                        pending_context_action =
                                            Some((key.clone(), ContextAction::MarkWaiting));
                                        ui.close_menu();
                                    }
                                    if ui
                                        .add_enabled(
                                            can_edit_locale
                                                && status == WAITING
                                                && row_valid
                                                && row_safe,
                                            egui::Button::new("Mark on review"),
                                        )
                                        .clicked()
                                    {
                                        pending_context_action =
                                            Some((key.clone(), ContextAction::MarkOnReview));
                                        ui.close_menu();
                                    }
                                    if ui
                                        .add_enabled(
                                            can_edit_locale
                                                && status == ON_REVIEW
                                                && row_valid
                                                && row_safe,
                                            egui::Button::new("Accept"),
                                        )
                                        .clicked()
                                    {
                                        pending_context_action =
                                            Some((key.clone(), ContextAction::Accept));
                                        ui.close_menu();
                                    }
                                    if ui
                                        .add_enabled(
                                            can_edit_locale && status != WAITING && row_safe,
                                            egui::Button::new("Reject"),
                                        )
                                        .clicked()
                                    {
                                        pending_context_action =
                                            Some((key.clone(), ContextAction::Reject));
                                        ui.close_menu();
                                    }
                                    if ui.button("Show usages").clicked() {
                                        pending_context_action =
                                            Some((key.clone(), ContextAction::ShowUsages));
                                        ui.close_menu();
                                    }
                                    if ui.button("Show metadata").clicked() {
                                        pending_context_action =
                                            Some((key.clone(), ContextAction::ShowMetadata));
                                        ui.close_menu();
                                    }
                                });
                            }
                        });
                });

            // Side panels must be laid out before the central panel so the editor
            // receives the actual remaining width instead of rendering underneath
            // the inspector on narrow windows.
            if self.show_inspector {
                egui::SidePanel::right("inspector")
                    .default_width(INSPECTOR_WIDTH)
                    .width_range(SIDE_PANEL_MIN_WIDTH..=SIDE_PANEL_MAX_WIDTH)
                    .resizable(true)
                    .frame(panel_frame(panel_background(), 12.0))
                    .show(context, |ui| {
                        constrain_side_panel_content(ui);
                        ui.heading("Inspector");
                        egui::ScrollArea::vertical()
                        .id_salt("inspector_scroll")
                        .max_width(ui.available_width())
                        .scroll_bar_visibility(
                            egui::containers::scroll_area::ScrollBarVisibility::VisibleWhenNeeded,
                        )
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            if let Some(view) = self.selected_view() {
                                status_label(ui, &view.status);
                                ui.label(&view.key);
                                ui.separator();
                                ui.strong("Validation");
                                let errors = if self.draft_dirty {
                                    self.store
                                        .validation_errors_for_text(&view.key, &self.draft)
                                } else {
                                    view.validation_errors.clone()
                                };
                                if errors.is_empty() {
                                    ui.colored_label(
                                        egui::Color32::from_rgb(92, 195, 124),
                                        "Valid for review",
                                    );
                                } else {
                                    for error in errors {
                                        ui.colored_label(
                                            egui::Color32::from_rgb(235, 150, 120),
                                            format!("• {error}"),
                                        );
                                    }
                                }
                                if view.metadata.orphaned {
                                    ui.colored_label(
                                        egui::Color32::from_rgb(235, 150, 120),
                                        "Orphaned key",
                                    );
                                }
                                ui.separator();
                                ui.strong("Metadata");
                                show_optional_metadata(
                                    ui,
                                    "Story",
                                    view.metadata.story_id.as_deref(),
                                );
                                show_optional_metadata(
                                    ui,
                                    "Quest",
                                    view.metadata.quest_id.as_deref(),
                                );
                                show_optional_metadata(
                                    ui,
                                    "Dialogue",
                                    view.metadata.dialogue_id.as_deref(),
                                );
                                show_optional_metadata(
                                    ui,
                                    "Owner",
                                    view.metadata.owner_id.as_deref(),
                                );
                                show_optional_metadata(
                                    ui,
                                    "Speaker",
                                    view.metadata.speaker_id.as_deref(),
                                );
                                show_optional_metadata(
                                    ui,
                                    "Translator",
                                    view.translated_by.as_deref(),
                                );
                                ui.label(format!("Source hash: {}", short_hash(&view.source_hash)));
                                ui.label(format!(
                                    "Translation hash: {}",
                                    short_hash(&view.text_hash)
                                ));
                                if view.status == ACCEPTED
                                    && view.source_hash != view.stored_source_hash
                                {
                                    ui.colored_label(
                                        egui::Color32::from_rgb(235, 150, 120),
                                        "Source hash is stale",
                                    );
                                }
                                ui.separator();
                                ui.strong("Usages");
                                for usage in usages(&view.metadata) {
                                    ui.label(format!("• {usage}"));
                                }
                                ui.separator();
                                ui.collapsing("Differences", |ui| {
                                    ui.label(format!(
                                        "English: {} characters",
                                        view.source_text.chars().count()
                                    ));
                                    ui.label(format!(
                                        "Translation: {} characters",
                                        self.draft.chars().count()
                                    ));
                                    ui.label(format!(
                                        "English lines: {}",
                                        line_count(&view.source_text)
                                    ));
                                    ui.label(format!(
                                        "Translation lines: {}",
                                        line_count(&self.draft)
                                    ));
                                });
                            } else {
                                ui.label("Select an entry to inspect its metadata and validation.");
                            }
                        });
                    });
            }

            egui::CentralPanel::default()
            .frame(panel_frame(app_background(), 16.0))
            .show(context, |ui| {
                if self.selected_view().is_none() {
                    let empty_rect = ui.available_rect_before_wrap();
                    ui.allocate_new_ui(
                        egui::UiBuilder::new()
                            .max_rect(empty_rect)
                            .layout(egui::Layout::top_down(egui::Align::Center)),
                        |ui| {
                            ui.set_width(empty_rect.width());
                            ui.add_space(80.0);
                            ui.label(egui::RichText::new("Select an entry").heading());
                            ui.add(
                                egui::Label::new("Choose an entry from the explorer to begin editing.")
                                    .wrap_mode(egui::TextWrapMode::Wrap)
                                    .halign(egui::Align::Center),
                            );
                        },
                    );
                    return;
                }
                egui::ScrollArea::vertical()
                    .id_salt("editor_scroll")
                    .scroll_bar_visibility(
                        egui::containers::scroll_area::ScrollBarVisibility::VisibleWhenNeeded,
                    )
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        let Some(view) = self.selected_view() else {
                            return;
                        };
                        ui.add(
                            egui::Label::new(egui::RichText::new(&view.key).heading())
                                .wrap_mode(egui::TextWrapMode::Wrap),
                        );
                        ui.horizontal_wrapped(|ui| {
                            status_label(ui, &view.status);
                            if self.draft_dirty {
                                ui.colored_label(egui::Color32::from_rgb(236, 173, 72), "● Unsaved changes");
                            }
                        });
                        ui.label(format!("{} · {}", view.metadata.entry_type, locale_name(&self.locale)));
                        ui.separator();

                        ui.strong("Source (English)");
                        let mut source_text = view.source_text.clone();
                        ui.add(
                            egui::TextEdit::multiline(&mut source_text)
                                .desired_rows(4)
                                .interactive(false)
                                .desired_width(f32::INFINITY),
                        );
                        editor_counts(ui, &view.source_text);

                        if self.locale == self.store.catalog.source_locale {
                            ui.colored_label(
                                egui::Color32::from_rgb(160, 180, 205),
                                "The source locale is read-only in this editor.",
                            );
                        } else if view.metadata.orphaned || view.source_hash.is_empty() {
                            ui.colored_label(
                                egui::Color32::from_rgb(235, 150, 120),
                                "This key is orphaned and cannot be edited until it is mapped to a source entry.",
                            );
                            let mut orphaned_text = self.draft.clone();
                            ui.add(
                                egui::TextEdit::multiline(&mut orphaned_text)
                                    .desired_rows(8)
                                    .desired_width(f32::INFINITY)
                                    .interactive(false),
                            );
                            editor_counts(ui, &self.draft);
                        } else {
                            ui.add_space(10.0);
                            ui.horizontal_wrapped(|ui| {
                                ui.strong(format!("Translation ({})", locale_name(&self.locale)));
                                status_label(ui, &view.status);
                            });
                            let changed = ui
                                .add(
                                    egui::TextEdit::multiline(&mut self.draft)
                                        .desired_rows(12)
                                        .desired_width(f32::INFINITY)
                                        .hint_text("Enter the translation. Variables and commands are validated before review."),
                                )
                                .changed();
                            if changed {
                                self.draft_dirty = true;
                            }
                            editor_counts(ui, &self.draft);
                            let draft_errors = self.store.validation_errors_for_text(&view.key, &self.draft);
                            if draft_errors.is_empty() {
                                ui.colored_label(egui::Color32::from_rgb(92, 195, 124), "Validation passed");
                            } else {
                                ui.colored_label(egui::Color32::from_rgb(235, 102, 102), "Validation needs attention");
                                for error in &draft_errors {
                                    ui.colored_label(egui::Color32::from_rgb(235, 150, 120), format!("• {error}"));
                                }
                            }
                            ui.horizontal_wrapped(|ui| {
                                let can_review = !self.draft_dirty
                                    && draft_errors.is_empty()
                                    && view.status == WAITING;
                                if ui
                                    .add_enabled(
                                        self.draft_dirty,
                                        egui::Button::new("Save").min_size(egui::vec2(92.0, 32.0)),
                                    )
                                    .clicked()
                                {
                                    pending_action = Some(EditorAction::Save);
                                }
                                if ui
                                    .add_enabled(
                                        can_review,
                                        egui::Button::new("Mark on review")
                                            .min_size(egui::vec2(132.0, 32.0)),
                                    )
                                    .clicked()
                                {
                                    pending_action = Some(EditorAction::MarkOnReview);
                                }
                                let can_accept = !self.draft_dirty
                                    && draft_errors.is_empty()
                                    && view.status == ON_REVIEW;
                                if ui
                                    .add_enabled(
                                        can_accept,
                                        egui::Button::new("Accept")
                                            .min_size(egui::vec2(92.0, 32.0)),
                                    )
                                    .clicked()
                                {
                                    pending_action = Some(EditorAction::Accept);
                                }
                                if ui
                                    .add_enabled(
                                        view.status != WAITING,
                                        egui::Button::new("Reject")
                                            .min_size(egui::vec2(92.0, 32.0)),
                                    )
                                    .clicked()
                                {
                                    pending_action = Some(EditorAction::Reject);
                                }
                                if ui
                                    .add_sized([92.0, 32.0], egui::Button::new("Reset"))
                                    .clicked()
                                {
                                    self.confirm_reset = true;
                                }
                            });
                        }

                        ui.separator();
                        ui.collapsing("Structured tokens and commands", |ui| {
                            let commands = commands_in_text(&view.source_text);
                            if view.metadata.variables.is_empty()
                                && view.metadata.tokens.is_empty()
                                && commands.is_empty()
                            {
                                ui.label("No structured tokens or commands registered.");
                            } else {
                                if !view.metadata.variables.is_empty() {
                                    ui.label(format!("Variables: {}", view.metadata.variables.join(", ")));
                                }
                                let token_values: Vec<String> = if view.metadata.tokens.is_empty() {
                                    commands.into_iter().collect()
                                } else {
                                    view.metadata.tokens.clone()
                                };
                                if !token_values.is_empty() {
                                    ui.label(format!("Commands: {}", token_values.join(", ")));
                                }
                                ui.label("These values must remain unchanged in translations.");
                            }
                        });
                        if self.show_metadata {
                            show_metadata(ui, &view);
                        }
                    });
            });

            if let Some((key, action)) = pending_context_action {
                if matches!(
                    action,
                    ContextAction::ShowUsages | ContextAction::ShowMetadata
                ) {
                    self.request_selection(key);
                    self.show_inspector = true;
                    if action == ContextAction::ShowMetadata {
                        self.show_metadata = true;
                    }
                } else {
                    self.apply_context_action(&key, action);
                }
            }
            if let Some(action) = pending_action {
                self.apply_action(action);
            }
            if self.confirm_reset {
                egui::Window::new("Reset translation?")
                .collapsible(false)
                .resizable(false)
                .show(context, |ui| {
                    ui.label("This clears the current translation and returns it to waiting for translation.");
                    ui.horizontal(|ui| {
                        if ui.button("Reset").clicked() {
                            self.confirm_reset = false;
                            self.apply_action(EditorAction::Reset);
                        }
                        if ui.button("Cancel").clicked() {
                            self.confirm_reset = false;
                        }
                    });
                });
            }
            if self.external_conflict {
                egui::Window::new("External changes detected")
                    .collapsible(false)
                    .resizable(false)
                    .show(context, |ui| {
                        ui.label("The catalog changed while you had unsaved edits.");
                        ui.horizontal_wrapped(|ui| {
                            if ui.button("Keep my changes").clicked() {
                                self.external_conflict = false;
                                self.external_snapshot = None;
                                self.notify(
                                "Kept local changes. Saving will overwrite the external version.",
                                true,
                            );
                            }
                            if ui.button("Reload external changes").clicked() {
                                if let Some(snapshot) = self.external_snapshot.take() {
                                    self.store = snapshot;
                                    self.draft_dirty = false;
                                    self.load_draft();
                                }
                                self.external_conflict = false;
                                self.notify("External changes reloaded.", false);
                            }
                            if ui.button("Compare").clicked() {
                                self.show_compare = true;
                            }
                        });
                    });
            }
            if self.show_compare {
                egui::Window::new("Compare external changes")
                    .resizable(true)
                    .show(context, |ui| {
                        ui.label("Your local draft");
                        let mut local_draft = self.draft.clone();
                        ui.add(
                            egui::TextEdit::multiline(&mut local_draft)
                                .desired_rows(8)
                                .interactive(false),
                        );
                        ui.separator();
                        ui.label("External translation");
                        let mut external = self
                            .external_snapshot
                            .as_ref()
                            .and_then(|store| {
                                self.selected
                                    .as_deref()
                                    .and_then(|key| store.entry_view(&self.locale, key))
                            })
                            .map(|view| view.target_text)
                            .unwrap_or_else(|| "No external value available.".into());
                        ui.add(
                            egui::TextEdit::multiline(&mut external)
                                .desired_rows(8)
                                .interactive(false),
                        );
                        if ui.button("Close comparison").clicked() {
                            self.show_compare = false;
                        }
                    });
            }
            if let Some(notification) = &self.notification {
                if notification.expires_at > Instant::now() {
                    egui::TopBottomPanel::bottom("notification")
                        .frame(panel_frame(toolbar_background(), 8.0))
                        .show(context, |ui| {
                            let color = if notification.is_error {
                                egui::Color32::from_rgb(235, 150, 120)
                            } else {
                                egui::Color32::from_rgb(133, 210, 157)
                            };
                            ui.colored_label(color, &notification.message);
                        });
                } else {
                    self.notification = None;
                }
            }
            context.request_repaint_after(Duration::from_millis(250));
        }
    }

    fn locale_name(locale: &str) -> &'static str {
        match locale {
            "en" => "English",
            "es" => "Español",
            _ => "Other",
        }
    }

    fn filter_combo_sized<'a, I>(
        ui: &mut egui::Ui,
        label: &str,
        value: &mut String,
        options: I,
        width: f32,
    ) where
        I: IntoIterator<Item = &'a str>,
    {
        // ComboBox::height controls the popup menu height, not the button height.
        // Keep the button in a fixed slot so columns align without forcing short
        // menus to show an unnecessary scrollbar.
        ui.allocate_ui_with_layout(
            egui::vec2(width, 30.0),
            egui::Layout::top_down(egui::Align::Center),
            |ui| {
                egui::ComboBox::from_id_salt(label)
                    .width(width)
                    .selected_text(if value.is_empty() {
                        format!("All {label}s")
                    } else {
                        format!("{label}: {value}")
                    })
                    .show_ui(ui, |ui| {
                        if ui
                            .selectable_value(value, String::new(), format!("All {label}s"))
                            .changed()
                        {
                            ui.memory_mut(|memory| memory.close_popup());
                        }
                        for option in options {
                            if ui
                                .selectable_value(value, option.to_string(), option)
                                .changed()
                            {
                                ui.memory_mut(|memory| memory.close_popup());
                            }
                        }
                    });
            },
        );
    }

    fn sort_combo(ui: &mut egui::Ui, value: &mut SortMode, width: f32) {
        ui.allocate_ui_with_layout(
            egui::vec2(width, 30.0),
            egui::Layout::top_down(egui::Align::Center),
            |ui| {
                egui::ComboBox::from_id_salt("sort_selector")
                    .width(width)
                    .selected_text(format!("Sort: {}", value.label()))
                    .show_ui(ui, |ui| {
                        for mode in [
                            SortMode::Key,
                            SortMode::Status,
                            SortMode::Story,
                            SortMode::Owner,
                        ] {
                            if ui.selectable_value(value, mode, mode.label()).changed() {
                                ui.memory_mut(|memory| memory.close_popup());
                            }
                        }
                    });
            },
        );
    }

    fn editor_counts(ui: &mut egui::Ui, text: &str) {
        ui.small(format!(
            "{} characters · {} lines",
            text.chars().count(),
            line_count(text)
        ));
    }

    fn line_count(text: &str) -> usize {
        text.lines().count().max(1)
    }

    fn truncate_for_node(text: &str, max_chars: usize) -> String {
        let mut chars = text.chars();
        let result: String = chars.by_ref().take(max_chars).collect();
        if chars.next().is_some() {
            format!("{result}…")
        } else {
            result
        }
    }

    fn short_hash(hash: &str) -> &str {
        hash.get(..12).unwrap_or(hash)
    }

    fn show_optional_metadata(ui: &mut egui::Ui, label: &str, value: Option<&str>) {
        ui.label(format!("{label}: {}", value.unwrap_or("—")));
    }

    fn show_metadata(ui: &mut egui::Ui, view: &EntryViewModel) {
        ui.collapsing("Metadata", |ui| {
            show_optional_metadata(ui, "Story", view.metadata.story_id.as_deref());
            show_optional_metadata(ui, "Quest", view.metadata.quest_id.as_deref());
            show_optional_metadata(ui, "Dialogue", view.metadata.dialogue_id.as_deref());
            show_optional_metadata(ui, "Owner", view.metadata.owner_id.as_deref());
            show_optional_metadata(ui, "Speaker", view.metadata.speaker_id.as_deref());
            show_optional_metadata(ui, "Translator", view.translated_by.as_deref());
            ui.label(format!("Source hash: {}", view.source_hash));
            ui.label(format!("Translation hash: {}", view.text_hash));
        });
    }

    fn usages(metadata: &KeyEntry) -> Vec<String> {
        let mut result = Vec::new();
        if let Some(story) = &metadata.story_id {
            result.push(format!("story: {story}"));
        }
        if let Some(quest) = &metadata.quest_id {
            result.push(format!("quest: {quest}"));
        }
        if let Some(dialogue) = &metadata.dialogue_id {
            result.push(format!("dialogue: {dialogue}"));
        }
        if let Some(owner) = &metadata.owner_id {
            result.push(format!("owner: {owner}"));
        }
        if result.is_empty() {
            result.push("No mapped usages".into());
        }
        result
    }
}

fn run_gui(root: PathBuf) -> Result<(), AppError> {
    dioxus_ui::run(root)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(feature = "legacy-egui")]
    use crate::legacy_egui::{ContentSection, ContentView, EditorApp, StoryReaderState};
    #[cfg(feature = "legacy-egui")]
    use std::sync::mpsc::channel;
    use tempfile::tempdir;

    #[test]
    fn hashes_are_stable() {
        assert_eq!(hash_text("hello"), hash_text("hello"));
        assert_ne!(hash_text("hello"), hash_text("hola"));
    }

    #[test]
    fn placeholders_are_extracted() {
        assert_eq!(
            placeholders("Craft {item} ({count})"),
            BTreeSet::from(["count".into(), "item".into()])
        );
    }

    fn test_store() -> (tempfile::TempDir, Store) {
        let directory = tempdir().unwrap();
        let root = directory.path().join("localization");
        fs::create_dir_all(root.join("locales")).unwrap();
        fs::create_dir_all(root.join("runtime")).unwrap();
        write_json(
            &root.join("keys.json"),
            &Catalog {
                schema_version: 1,
                source_locale: "en".into(),
                target_locales: vec!["es".into()],
                entries: BTreeMap::new(),
            },
        )
        .unwrap();
        write_json(
            &root.join("locales/en.json"),
            &LocaleFile {
                schema_version: CATALOG_SCHEMA_VERSION,
                locale: "en".into(),
                entries: BTreeMap::from([(
                    "dialog.test".into(),
                    LocaleEntry {
                        text: "Hello {name}".into(),
                        status: "source".into(),
                        source_hash: hash_text("Hello {name}"),
                        text_hash: hash_text("Hello {name}"),
                        translated_by: None,
                    },
                )]),
            },
        )
        .unwrap();
        write_json(
            &root.join("locales/es.json"),
            &LocaleFile {
                schema_version: CATALOG_SCHEMA_VERSION,
                locale: "es".into(),
                entries: BTreeMap::from([(
                    "dialog.test".into(),
                    LocaleEntry {
                        text: "Hola {name}".into(),
                        status: ACCEPTED.into(),
                        source_hash: hash_text("Hello {name}"),
                        text_hash: hash_text("Hola {name}"),
                        translated_by: None,
                    },
                )]),
            },
        )
        .unwrap();
        let store = Store::load(&root).unwrap();
        (directory, store)
    }

    #[test]
    fn sync_creates_metadata_and_marks_changed_source_waiting() {
        let (_directory, mut store) = test_store();
        store.source.entries.get_mut("dialog.test").unwrap().text = "Welcome {name}".into();
        let report = store.sync().unwrap();
        assert_eq!(report.created_keys, 1);
        assert_eq!(report.changed_sources, 1);
        assert_eq!(store.targets["es"].entries["dialog.test"].status, WAITING);
    }

    #[test]
    fn sync_marks_manual_translation_edit_waiting() {
        let (_directory, mut store) = test_store();
        store
            .targets
            .get_mut("es")
            .unwrap()
            .entries
            .get_mut("dialog.test")
            .unwrap()
            .text = "Hola, {name}".into();
        let report = store.sync().unwrap();
        assert_eq!(report.changed_manual_translations, 1);
        assert_eq!(store.targets["es"].entries["dialog.test"].status, WAITING);
    }

    #[test]
    fn quest_dialog_command_lines_have_distinct_authoring_node_types() {
        let (_directory, mut store) = test_store();
        store.content.items.insert(
            "HAMMER".into(),
            content::ItemDefinition {
                name_key: String::new(),
                description_key: String::new(),
                category: "tool".into(),
                sprite_ref: String::new(),
                stackable: false,
                max_stack: Some(1),
                effects: Vec::new(),
                tags: Vec::new(),
                quest_item: false,
            },
        );
        let (item_node, item_action) =
            command_node_definition("[SHOW:HAMMER]", &store.content.items);
        assert_eq!(item_node, "show_item");
        assert!(matches!(
            item_action,
            content::NodePayload::ShowItem { item_id } if item_id == "HAMMER"
        ));
        let (icon_node, _) = command_node_definition("[SHOW:LOVE]", &store.content.items);
        assert_eq!(icon_node, "show_icon");
        let (action_node, action) =
            command_node_definition("[START_CAMERA_SHAKE]", &store.content.items);
        assert_eq!(action_node, "action");
        assert!(matches!(
            action,
            content::NodePayload::Action(content::ActionDefinition::CameraShakeStart)
        ));
    }

    #[test]
    fn quest_dialog_scene_migration_creates_one_node_per_line_and_is_idempotent() {
        let (_directory, mut store) = test_store();
        store.content.items.insert(
            "HAMMER".into(),
            content::ItemDefinition {
                name_key: String::new(),
                description_key: String::new(),
                category: "tool".into(),
                sprite_ref: String::new(),
                stackable: false,
                max_stack: Some(1),
                effects: Vec::new(),
                tags: Vec::new(),
                quest_item: false,
            },
        );
        let scene_ids = [
            "npc_blacksmith",
            "nice_lamp",
            "save_blacksmith_01",
            "blacksmith_city_help",
            "blacksmith_missing_hammer",
            "blacksmith_found_hammer",
            "blacksmith_idle",
        ];
        for scene_id in scene_ids {
            let key = format!("dialog.{scene_id}.hello");
            store.source.entries.insert(
                key.clone(),
                LocaleEntry {
                    text: "Hello!".into(),
                    status: "source".into(),
                    source_hash: hash_text("Hello!"),
                    text_hash: hash_text("Hello!"),
                    translated_by: None,
                },
            );
            store.content.dialogues.insert(
                scene_id.into(),
                content::DialogueDefinition {
                    name_key: String::new(),
                    story_id: Some("blacksmith".into()),
                    chapter_id: None,
                    owner_id: Some("npc.blacksmith".into()),
                    lines: vec![content::DialogueLineDefinition {
                        id: key.clone(),
                        kind: "text".into(),
                        key: Some(key),
                        command: None,
                        speaker: Some("Npc".into()),
                        variables: Vec::new(),
                        on_enter: None,
                        on_exit: None,
                    }],
                    on_enter: Vec::new(),
                    on_exit: Vec::new(),
                },
            );
        }
        let npc = store.content.dialogues.get_mut("npc_blacksmith").unwrap();
        npc.lines.push(content::DialogueLineDefinition {
            id: "npc_blacksmith.show_hammer".into(),
            kind: "command".into(),
            key: None,
            command: Some("[SHOW:HAMMER]".into()),
            speaker: Some("Npc".into()),
            variables: Vec::new(),
            on_enter: None,
            on_exit: None,
        });
        let created = store.migrate_quest_dialog_scenes().unwrap();
        assert_eq!(created.len(), 7);
        let story = &store.content.stories["blacksmith.npc_blacksmith"];
        assert_eq!(story.nodes.len(), 2);
        assert_eq!(story.nodes[&story.entry_node_id].node_type, "dialogue");
        assert!(story
            .nodes
            .values()
            .any(|node| node.node_type == "show_item"));
        assert_eq!(
            store.content.dialogues["npc_blacksmith"]
                .story_id
                .as_deref(),
            Some("blacksmith.npc_blacksmith")
        );
        assert!(store.migrate_quest_dialog_scenes().unwrap().is_empty());
    }

    #[test]
    fn opening_quest_dialog_migration_creates_six_independent_stories() {
        let (_directory, mut store) = test_store();
        let scenes = [
            ("init_text_0", "intro"),
            ("init_text_1", "intro"),
            ("init_pre_board", "intro"),
            ("init_board", "workbench"),
            ("after_workbench_repair", "workbench"),
            ("well_repair_quest", "well"),
        ];

        for (dialogue_id, old_story_id) in scenes {
            let key = format!("dialog.{dialogue_id}.hello");
            let text = format!("Hello from {dialogue_id}");
            let hash = hash_text(&text);
            store.source.entries.insert(
                key.clone(),
                LocaleEntry {
                    text,
                    status: "source".into(),
                    source_hash: hash.clone(),
                    text_hash: hash,
                    translated_by: None,
                },
            );
            store.catalog.entries.insert(
                key.clone(),
                KeyEntry {
                    entry_type: "dialogue_line".into(),
                    story_id: Some(old_story_id.into()),
                    dialogue_id: Some(dialogue_id.into()),
                    owner_id: Some(format!("story.{old_story_id}")),
                    speaker_id: Some("Player".into()),
                    ..Default::default()
                },
            );
            store.content.dialogues.insert(
                dialogue_id.into(),
                content::DialogueDefinition {
                    name_key: String::new(),
                    story_id: Some(old_story_id.into()),
                    chapter_id: None,
                    owner_id: Some(format!("story.{old_story_id}")),
                    lines: vec![content::DialogueLineDefinition {
                        id: format!("{dialogue_id}.line.0"),
                        kind: "text".into(),
                        key: Some(key),
                        command: None,
                        speaker: Some("Player".into()),
                        variables: Vec::new(),
                        on_enter: None,
                        on_exit: None,
                    }],
                    on_enter: Vec::new(),
                    on_exit: Vec::new(),
                },
            );
        }

        let created = store.migrate_opening_quest_dialog_scenes().unwrap();
        assert_eq!(created.len(), 6);
        for (dialogue_id, _) in scenes {
            let story_id = format!("quest_dialog.{dialogue_id}");
            let story = &store.content.stories[&story_id];
            assert_eq!(story.nodes.len(), 1);
            assert_eq!(story.entry_node_id, format!("{story_id}.line_0"));
            assert_eq!(
                store.content.dialogues[dialogue_id].story_id.as_deref(),
                Some(story_id.as_str())
            );
            assert!(store.source.entries.contains_key(&story.name_key));
            assert_eq!(store.targets["es"].entries[&story.name_key].status, WAITING);
        }
        assert!(store
            .migrate_opening_quest_dialog_scenes()
            .unwrap()
            .is_empty());
    }

    #[test]
    fn creating_a_story_is_atomic_and_creates_pending_translation_keys() {
        let (_directory, mut store) = test_store();
        let created = store.create_story().unwrap();
        assert!(store.content.stories.contains_key(&created.story_id));
        assert_eq!(
            store.content.stories[&created.story_id].entry_node_id,
            created.entry_node_id
        );
        assert!(store.content.dialogues.contains_key(&created.dialogue_id));
        for key in [
            format!("story.{}.name", created.story_id),
            format!("story.{}.description", created.story_id),
            format!("dialog.{}.intro.start", created.story_id),
        ] {
            assert!(store.source.entries.contains_key(&key));
            assert_eq!(store.targets["es"].entries[&key].status, WAITING);
        }
    }

    #[test]
    fn story_display_name_changes_do_not_change_stable_story_id() {
        let (_directory, mut store) = test_store();
        let created = store.create_story().unwrap();
        store
            .content
            .stories
            .get_mut(&created.story_id)
            .unwrap()
            .name_key = "story.renamed.name".into();
        assert!(store.content.stories.contains_key(&created.story_id));
    }

    #[test]
    fn creating_nodes_creates_dialogue_keys_and_auto_connects_selected_node() {
        let (_directory, mut store) = test_store();
        let story = store.create_story().unwrap();
        let first = store
            .create_node(
                &story.story_id,
                "dialogue",
                "Blacksmith",
                "Bring {item}",
                None,
                None,
            )
            .unwrap();
        assert!(first.dialogue_id.is_some());
        assert_eq!(
            store.targets["es"].entries[&first.line_key.clone().unwrap()].status,
            WAITING
        );

        let second = store
            .create_node(
                &story.story_id,
                "dialogue",
                "Blacksmith",
                "Here is the {item}.",
                None,
                Some((first.node_id.clone(), "default".into(), "next".into())),
            )
            .unwrap();
        let edge_id = second.connection_id.clone().unwrap();
        let current = &store.content.stories[&story.story_id];
        assert_eq!(current.connections[&edge_id].from_node_id, first.node_id);
        assert_eq!(current.connections[&edge_id].to_node_id, second.node_id);
        assert!(store.source.entries.contains_key(&second.line_key.unwrap()));
    }

    #[test]
    fn connection_commands_reject_duplicates_and_remove_edges_with_nodes() {
        let (_directory, mut store) = test_store();
        let story = store.create_story().unwrap();
        let first = store
            .create_node(&story.story_id, "dialogue", "A", "One", None, None)
            .unwrap();
        let second = store
            .create_node(&story.story_id, "dialogue", "B", "Two", None, None)
            .unwrap();
        let request = ConnectionRequest {
            from_node_id: first.node_id.clone(),
            to_node_id: second.node_id.clone(),
            output_id: "default".into(),
            connection_kind: "next".into(),
            label_key: None,
            condition: None,
        };
        let edge = store
            .connect_nodes(&story.story_id, request.clone())
            .unwrap();
        assert!(store.connect_nodes(&story.story_id, request).is_err());
        store.delete_connection(&story.story_id, &edge).unwrap();
        let edge = store
            .connect_nodes(
                &story.story_id,
                ConnectionRequest {
                    from_node_id: first.node_id.clone(),
                    to_node_id: second.node_id.clone(),
                    output_id: "default".into(),
                    connection_kind: "next".into(),
                    label_key: None,
                    condition: None,
                },
            )
            .unwrap();
        store.delete_node(&story.story_id, &second.node_id).unwrap();
        assert!(!store.content.stories[&story.story_id]
            .connections
            .contains_key(&edge));
    }

    #[cfg(feature = "legacy-egui")]
    #[test]
    fn reader_navigation_supports_linear_history_and_restart() {
        let mut reader = StoryReaderState::default();
        reader.restart_at("story".into(), "start".into(), "en".into());
        reader.advance_to("middle".into());
        reader.advance_to("end".into());
        assert_eq!(reader.current_node_id.as_deref(), Some("end"));
        reader.back();
        assert_eq!(reader.current_node_id.as_deref(), Some("middle"));
        reader.restart_at("story".into(), "start".into(), "es".into());
        assert!(reader.history.is_empty());
        assert_eq!(reader.locale, "es");
    }

    #[cfg(feature = "legacy-egui")]
    #[test]
    fn reader_preview_uses_accepted_locale_and_falls_back_for_pending_text() {
        let (_directory, store) = test_store();
        let (_sender, receiver) = channel();
        let mut app = EditorApp {
            root: PathBuf::new(),
            store,
            locale: "es".into(),
            filters: FilterState::default(),
            show_content: true,
            show_studio: true,
            show_inspector: true,
            show_metadata: false,
            selected: None,
            draft: String::new(),
            draft_dirty: false,
            notification: None,
            external_change: receiver,
            external_snapshot: None,
            external_conflict: false,
            show_compare: false,
            confirm_reset: false,
            selected_story_id: None,
            selected_node_id: None,
            content_search: String::new(),
            content_view: ContentView::Reader,
            content_dirty: false,
            content_section: ContentSection::Stories,
            selected_content_id: None,
            reader: StoryReaderState::default(),
        };
        let accepted = app.localized_preview("dialog.test");
        assert_eq!(accepted.text, "Hola {name}");
        assert!(!accepted.used_fallback);
        app.store
            .targets
            .get_mut("es")
            .unwrap()
            .entries
            .get_mut("dialog.test")
            .unwrap()
            .status = WAITING.into();
        let fallback = app.localized_preview("dialog.test");
        assert_eq!(fallback.text, "Hello {name}");
        assert!(fallback.used_fallback);
        assert_eq!(fallback.status, WAITING);
    }

    #[test]
    fn validation_requires_exact_placeholders_and_line_breaks() {
        let directory = tempdir().unwrap();
        let root = directory.path().to_path_buf();
        fs::create_dir_all(root.join("locales")).unwrap();
        fs::create_dir_all(root.join("runtime")).unwrap();
        write_json(
            &root.join("locales/en.json"),
            &LocaleFile {
                schema_version: CATALOG_SCHEMA_VERSION,
                locale: "en".into(),
                entries: BTreeMap::from([(
                    "dialog.test".into(),
                    LocaleEntry {
                        text: "Hello {name}\\nWelcome".into(),
                        status: "source".into(),
                        source_hash: String::new(),
                        text_hash: String::new(),
                        translated_by: None,
                    },
                )]),
            },
        )
        .unwrap();
        write_json(
            &root.join("locales/es.json"),
            &LocaleFile {
                schema_version: CATALOG_SCHEMA_VERSION,
                locale: "es".into(),
                entries: BTreeMap::from([(
                    "dialog.test".into(),
                    LocaleEntry {
                        text: "Hola {name} y {extra}".into(),
                        status: WAITING.into(),
                        source_hash: String::new(),
                        text_hash: String::new(),
                        translated_by: None,
                    },
                )]),
            },
        )
        .unwrap();
        write_json(
            &root.join("keys.json"),
            &Catalog {
                schema_version: 1,
                source_locale: "en".into(),
                target_locales: vec!["es".into()],
                entries: BTreeMap::from([("dialog.test".into(), KeyEntry::default())]),
            },
        )
        .unwrap();
        let store = Store::load(root).unwrap();
        assert!(store.validate().is_err());
    }

    #[test]
    fn combined_filters_and_sorting_return_only_matching_entries() {
        let (_directory, mut store) = test_store();
        store.catalog.entries.insert(
            "dialog.test".into(),
            KeyEntry {
                entry_type: "dialogue_line".into(),
                story_id: Some("intro".into()),
                quest_id: Some("first_steps".into()),
                owner_id: Some("guide".into()),
                speaker_id: Some("npc_guide".into()),
                ..Default::default()
            },
        );
        store.source.entries.insert(
            "quest.second".into(),
            LocaleEntry {
                text: "Find the hammer".into(),
                status: "source".into(),
                source_hash: hash_text("Find the hammer"),
                text_hash: hash_text("Find the hammer"),
                translated_by: None,
            },
        );
        store.catalog.entries.insert(
            "quest.second".into(),
            KeyEntry {
                entry_type: "quest_description".into(),
                story_id: Some("intro".into()),
                quest_id: Some("first_steps".into()),
                owner_id: Some("quest_system".into()),
                ..Default::default()
            },
        );
        store.targets.get_mut("es").unwrap().entries.insert(
            "quest.second".into(),
            LocaleEntry {
                text: "Encuentra el martillo".into(),
                status: ON_REVIEW.into(),
                source_hash: hash_text("Find the hammer"),
                text_hash: hash_text("Encuentra el martillo"),
                translated_by: Some("test".into()),
            },
        );

        let mut filters = FilterState {
            search: "intro".into(),
            status: ON_REVIEW.into(),
            story: "intro".into(),
            quest: "first_steps".into(),
            sort_mode: SortMode::Status,
            ..Default::default()
        };
        assert_eq!(store.filtered_keys("es", &filters), vec!["quest.second"]);

        filters.search = "hammer".into();
        assert_eq!(store.filtered_keys("es", &filters), vec!["quest.second"]);
        filters.entry_type = "dialogue_line".into();
        assert!(store.filtered_keys("es", &filters).is_empty());
    }

    #[test]
    fn source_locale_filters_and_sorting_use_source_status() {
        let (_directory, mut store) = test_store();
        store.sync().unwrap();
        let filters = FilterState {
            status: "source".into(),
            sort_mode: SortMode::Status,
            ..Default::default()
        };
        assert_eq!(store.filtered_keys("en", &filters), vec!["dialog.test"]);
    }

    #[test]
    fn effective_translation_status_counts_stale_accepted_text_as_waiting() {
        let (_directory, mut store) = test_store();
        store.sync().unwrap();
        for (key, source_text, target_text, status, source_hash) in [
            (
                "dialog.review",
                "Review this line",
                "Revisar esta línea",
                ON_REVIEW,
                hash_text("Review this line"),
            ),
            (
                "dialog.stale",
                "Updated source",
                "Fuente antigua",
                ACCEPTED,
                hash_text("Old source"),
            ),
        ] {
            store.catalog.entries.insert(
                key.into(),
                KeyEntry {
                    entry_type: "dialogue_line".into(),
                    ..Default::default()
                },
            );
            store.source.entries.insert(
                key.into(),
                LocaleEntry {
                    text: source_text.into(),
                    status: "source".into(),
                    source_hash: hash_text(source_text),
                    text_hash: hash_text(source_text),
                    translated_by: None,
                },
            );
            store.targets.get_mut("es").unwrap().entries.insert(
                key.into(),
                LocaleEntry {
                    text: target_text.into(),
                    status: status.into(),
                    source_hash,
                    text_hash: hash_text(target_text),
                    translated_by: Some("test".into()),
                },
            );
        }

        let counts = store.status_counts("es");
        assert_eq!(counts.get(ACCEPTED), Some(&1));
        assert_eq!(counts.get(ON_REVIEW), Some(&1));
        assert_eq!(counts.get(WAITING), Some(&1));
        let waiting = store.filtered_keys(
            "es",
            &FilterState {
                status: WAITING.into(),
                ..Default::default()
            },
        );
        assert_eq!(waiting, vec!["dialog.stale"]);
    }

    #[test]
    fn source_locale_entry_views_are_consistent_with_source_status() {
        let (_directory, mut store) = test_store();
        store.sync().unwrap();
        let view = store.entry_view("en", "dialog.test").unwrap();
        assert_eq!(view.status, "source");
        assert_eq!(view.target_text, "Hello {name}");
        assert_eq!(view.source_hash, view.stored_source_hash);
        assert_eq!(view.text_hash, view.source_hash);
        assert!(view.translated_by.is_none());
        assert!(view.validation_errors.is_empty());
    }

    #[cfg(feature = "legacy-egui")]
    #[test]
    fn loading_source_locale_keeps_source_text_available_to_inspector() {
        let (_directory, store) = test_store();
        let (_sender, receiver) = channel();
        let mut app = EditorApp {
            root: PathBuf::new(),
            store,
            locale: "en".into(),
            filters: FilterState::default(),
            show_content: true,
            show_studio: false,
            show_inspector: true,
            show_metadata: false,
            selected: Some("dialog.test".into()),
            draft: String::new(),
            draft_dirty: false,
            notification: None,
            external_change: receiver,
            external_snapshot: None,
            external_conflict: false,
            show_compare: false,
            confirm_reset: false,
            selected_story_id: None,
            selected_node_id: None,
            content_search: String::new(),
            content_view: ContentView::Graph,
            content_dirty: false,
            content_section: ContentSection::Stories,
            selected_content_id: None,
            reader: StoryReaderState::default(),
        };
        app.load_draft();
        assert_eq!(app.draft, "Hello {name}");
        assert!(!app.draft_dirty);
    }

    #[test]
    fn manual_save_marks_translation_waiting_and_accept_requires_current_valid_text() {
        let (_directory, mut store) = test_store();
        store
            .save_translation("es", "dialog.test", "Hola {name}".into())
            .unwrap();
        assert_eq!(store.targets["es"].entries["dialog.test"].status, WAITING);
        store.mark_on_review("es", "dialog.test").unwrap();
        assert_eq!(store.targets["es"].entries["dialog.test"].status, ON_REVIEW);
        store.accept("es", "dialog.test").unwrap();
        assert_eq!(store.targets["es"].entries["dialog.test"].status, ACCEPTED);
    }

    #[cfg(feature = "legacy-egui")]
    #[test]
    fn dirty_draft_blocks_selection_changes() {
        let (_directory, store) = test_store();
        let (_sender, receiver) = channel();
        let mut app = EditorApp {
            root: PathBuf::new(),
            store,
            locale: "es".into(),
            filters: FilterState::default(),
            show_content: true,
            show_studio: false,
            show_inspector: true,
            show_metadata: false,
            selected: Some("dialog.test".into()),
            draft: "Local draft".into(),
            draft_dirty: true,
            notification: None,
            external_change: receiver,
            external_snapshot: None,
            external_conflict: false,
            show_compare: false,
            confirm_reset: false,
            selected_story_id: None,
            selected_node_id: None,
            content_search: String::new(),
            content_view: ContentView::Graph,
            content_dirty: false,
            content_section: ContentSection::Stories,
            selected_content_id: None,
            reader: StoryReaderState::default(),
        };
        app.request_selection("dialog.other".into());
        assert_eq!(app.selected.as_deref(), Some("dialog.test"));
        assert_eq!(app.draft, "Local draft");
        assert!(app.notification.is_some());
    }

    #[test]
    fn orphaned_entries_remain_inspectable_in_the_explorer() {
        let (_directory, mut store) = test_store();
        store.catalog.entries.insert(
            "dialog.orphan".into(),
            KeyEntry {
                entry_type: "dialogue_line".into(),
                orphaned: true,
                story_id: Some("legacy".into()),
                ..Default::default()
            },
        );
        let view = store.entry_view("es", "dialog.orphan").unwrap();
        assert!(view.source_text.is_empty());
        assert!(view
            .validation_errors
            .iter()
            .any(|error| error.contains("unknown source key")));
    }

    #[test]
    fn commands_are_validated_as_non_translatable_tokens() {
        assert_eq!(
            commands_in_text("Use [SHOW:HAMMER] now [START_CAMERA_SHAKE]"),
            BTreeSet::from(["[SHOW:HAMMER]".into(), "[START_CAMERA_SHAKE]".into()])
        );
        let (_directory, mut store) = test_store();
        store.source.entries.get_mut("dialog.test").unwrap().text =
            "Hello [SHOW:HAMMER] {name}".into();
        assert!(store
            .save_translation("es", "dialog.test", "Hola {name}".into())
            .is_err());
        store
            .save_translation("es", "dialog.test", "Hola [SHOW:HAMMER] {name}".into())
            .unwrap();
    }

    #[test]
    fn filtering_scales_to_large_catalogs() {
        let mut entries = BTreeMap::new();
        let mut source_entries = BTreeMap::new();
        let mut target_entries = BTreeMap::new();
        for index in 0..10_000 {
            let key = format!("dialog.large_{index:05}");
            let text = format!("Line {index}");
            let hash = hash_text(&text);
            entries.insert(
                key.clone(),
                KeyEntry {
                    entry_type: "dialogue_line".into(),
                    story_id: Some(if index % 2 == 0 { "even" } else { "odd" }.into()),
                    ..Default::default()
                },
            );
            source_entries.insert(
                key.clone(),
                LocaleEntry {
                    text: text.clone(),
                    status: "source".into(),
                    source_hash: hash.clone(),
                    text_hash: hash.clone(),
                    translated_by: None,
                },
            );
            target_entries.insert(
                key,
                LocaleEntry {
                    text: String::new(),
                    status: WAITING.into(),
                    source_hash: hash,
                    text_hash: hash_text(""),
                    translated_by: None,
                },
            );
        }
        let store = Store {
            root: PathBuf::new(),
            paths: project::ProjectPaths {
                manifest_path: None,
                project_dir: PathBuf::new(),
                project_id: "test".into(),
                keys: PathBuf::new(),
                stories: PathBuf::new(),
                dialogues: PathBuf::new(),
                quests: PathBuf::new(),
                items: PathBuf::new(),
                entities: PathBuf::new(),
                callbacks: PathBuf::new(),
                locales: PathBuf::new(),
                export: PathBuf::new(),
            },
            catalog: Catalog {
                schema_version: 1,
                source_locale: "en".into(),
                target_locales: vec!["es".into()],
                entries,
            },
            source: LocaleFile {
                schema_version: CATALOG_SCHEMA_VERSION,
                locale: "en".into(),
                entries: source_entries,
            },
            targets: BTreeMap::from([(
                "es".into(),
                LocaleFile {
                    schema_version: CATALOG_SCHEMA_VERSION,
                    locale: "es".into(),
                    entries: target_entries,
                },
            )]),
            content: content::ContentCatalog::default(),
        };
        let filters = FilterState {
            story: "even".into(),
            ..Default::default()
        };
        assert_eq!(store.filtered_keys("es", &filters).len(), 5_000);
    }
}
