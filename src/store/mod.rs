use super::{
    AppError, EntryViewModel, FilterState, SortMode, ACCEPTED, CATALOG_SCHEMA_VERSION, ON_REVIEW,
    WAITING,
};
use crate::{content, project};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Catalog {
    pub(crate) schema_version: u32,
    pub(crate) source_locale: String,
    pub(crate) target_locales: Vec<String>,
    #[serde(default)]
    pub(crate) entries: BTreeMap<String, KeyEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub(crate) struct KeyEntry {
    #[serde(rename = "type")]
    pub(crate) entry_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) story_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) quest_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) dialogue_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) owner_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) speaker_id: Option<String>,
    #[serde(default)]
    pub(crate) variables: Vec<String>,
    #[serde(default)]
    pub(crate) tokens: Vec<String>,
    #[serde(default)]
    pub(crate) orphaned: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct LocaleFile {
    #[serde(default = "default_catalog_schema_version")]
    pub(crate) schema_version: u32,
    pub(crate) locale: String,
    #[serde(default)]
    pub(crate) entries: BTreeMap<String, LocaleEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct LocaleEntry {
    pub(crate) text: String,
    pub(crate) status: String,
    pub(crate) source_hash: String,
    pub(crate) text_hash: String,
    #[serde(default)]
    pub(crate) translated_by: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct RuntimeFile {
    #[serde(default = "default_catalog_schema_version")]
    pub(crate) schema_version: u32,
    pub(crate) locale: String,
    pub(crate) entries: BTreeMap<String, String>,
}

fn default_catalog_schema_version() -> u32 {
    CATALOG_SCHEMA_VERSION
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct DialogueCatalog {
    pub(crate) dialogues: BTreeMap<String, DialogueDefinition>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct DialogueDefinition {
    #[serde(default)]
    pub(crate) story_id: Option<String>,
    #[serde(default)]
    pub(crate) owner_id: Option<String>,
    pub(crate) lines: Vec<DialogueLineDefinition>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct DialogueLineDefinition {
    pub(crate) kind: String,
    #[serde(default)]
    pub(crate) key: Option<String>,
    #[serde(default)]
    pub(crate) speaker: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct QuestCatalog {
    pub(crate) quests: BTreeMap<String, QuestDefinition>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct QuestDefinition {
    #[serde(default)]
    pub(crate) story_id: Option<String>,
    #[serde(default)]
    pub(crate) owner_id: Option<String>,
    #[serde(default)]
    pub(crate) description_keys: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub(crate) struct SyncReport {
    pub(crate) created_keys: usize,
    pub(crate) created_translations: usize,
    pub(crate) changed_sources: usize,
    pub(crate) changed_manual_translations: usize,
    pub(crate) orphaned_translations: usize,
    pub(crate) orphaned_keys: usize,
}

#[derive(Debug, Clone)]
pub(crate) struct Store {
    pub(crate) root: PathBuf,
    pub(crate) paths: project::ProjectPaths,
    pub(crate) catalog: Catalog,
    pub(crate) source: LocaleFile,
    pub(crate) targets: BTreeMap<String, LocaleFile>,
    pub(crate) content: content::ContentCatalog,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CreatedStory {
    pub(crate) story_id: String,
    pub(crate) entry_node_id: String,
    pub(crate) dialogue_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CreatedNode {
    pub(crate) story_id: String,
    pub(crate) node_id: String,
    pub(crate) dialogue_id: Option<String>,
    pub(crate) line_key: Option<String>,
    pub(crate) connection_id: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct ConnectionRequest {
    pub(crate) from_node_id: String,
    pub(crate) to_node_id: String,
    pub(crate) output_id: String,
    pub(crate) connection_kind: String,
    pub(crate) label_key: Option<String>,
    pub(crate) condition: Option<content::ConditionDefinition>,
}

mod content_ops;
mod persistence;
mod sync;
mod translations;

fn infer_type(key: &str) -> String {
    if key.starts_with("dialog.") {
        "dialogue_line".into()
    } else if key.starts_with("quest.") {
        "quest_description".into()
    } else {
        "ui".into()
    }
}

pub(crate) fn next_content_id<'a>(prefix: &str, ids: impl Iterator<Item = &'a String>) -> String {
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

pub(crate) fn placeholders(text: &str) -> BTreeSet<String> {
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

pub(crate) fn commands_in_text(text: &str) -> BTreeSet<String> {
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

pub(crate) fn hash_text(text: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    format!("{:x}", hasher.finalize())
}

pub(crate) fn command_node_definition(
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

pub(crate) fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), AppError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let text = serde_json::to_string_pretty(value).map_err(|source| AppError::Json {
        path: path.display().to_string(),
        source,
    })?;

    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("kstory");
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let mut temp_path = None;
    let mut temp_file = None;
    for attempt in 0..100u32 {
        let candidate = path.with_file_name(format!(".{file_name}.{stamp}.{attempt}.tmp"));
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(file) => {
                temp_path = Some(candidate);
                temp_file = Some(file);
                break;
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.into()),
        }
    }
    let temp_path = temp_path.ok_or_else(|| {
        AppError::Validation(format!(
            "could not allocate temporary file for {}",
            path.display()
        ))
    })?;
    let mut temp_file = temp_file.expect("temporary file is present when its path is present");
    let write_result = (|| -> Result<(), AppError> {
        temp_file.write_all(format!("{text}\n").as_bytes())?;
        temp_file.sync_all()?;
        Ok(())
    })();
    drop(temp_file);
    if let Err(error) = write_result {
        let _ = fs::remove_file(&temp_path);
        return Err(error);
    }

    #[cfg(windows)]
    if path.exists() {
        // shortcut: Windows std::fs has no atomic replace; keep writes non-truncating until a native replacement API is needed.
        fs::remove_file(path)?;
    }
    if let Err(error) = fs::rename(&temp_path, path) {
        let _ = fs::remove_file(&temp_path);
        return Err(error.into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn write_json_replaces_existing_file_without_leaving_temporary_files() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("catalog.json");
        write_json(&path, &BTreeMap::from([("key", "first")])).unwrap();
        write_json(&path, &BTreeMap::from([("key", "second")])).unwrap();

        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("second"));
        assert!(!directory.path().read_dir().unwrap().any(|entry| entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .ends_with(".tmp")));
    }
}
