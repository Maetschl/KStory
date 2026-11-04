use super::*;

impl Store {
    pub(crate) fn list(&self, locale: &str, status: &str) -> Vec<(String, String)> {
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

    pub(crate) fn validation_errors_for_text(&self, key: &str, text: &str) -> Vec<String> {
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

    pub(crate) fn expected_commands(&self, key: &str, source_text: &str) -> BTreeSet<String> {
        let mut commands = commands_in_text(source_text);
        if let Some(metadata) = self.catalog.entries.get(key) {
            commands.extend(metadata.tokens.iter().cloned());
        }
        commands
    }

    pub(crate) fn validate_entry(&self, locale: &str, key: &str) -> Vec<String> {
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

    pub(crate) fn entry_view(&self, locale: &str, key: &str) -> Option<EntryViewModel> {
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

    pub(crate) fn localized_preview(&self, locale: &str, key: &str) -> (String, bool, String) {
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

    pub(crate) fn effective_translation_status(&self, locale: &str, key: &str) -> &'static str {
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

    pub(crate) fn status_counts(&self, locale: &str) -> BTreeMap<String, usize> {
        let mut counts = BTreeMap::new();
        for key in self.catalog.entries.keys() {
            let status = self.effective_translation_status(locale, key);
            *counts.entry(status.to_string()).or_default() += 1;
        }
        counts
    }

    pub(crate) fn filtered_keys(&self, locale: &str, filters: &FilterState) -> Vec<String> {
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

    pub(crate) fn save_translation(
        &mut self,
        locale: &str,
        key: &str,
        text: String,
    ) -> Result<(), AppError> {
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
        self.save_if_enabled()
    }

    pub(crate) fn mark_on_review(&mut self, locale: &str, key: &str) -> Result<(), AppError> {
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
        self.save_if_enabled()
    }

    #[allow(dead_code, reason = "reset is exposed by the legacy editor")]
    pub(crate) fn reset_translation(&mut self, locale: &str, key: &str) -> Result<(), AppError> {
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
        self.save_if_enabled()
    }

    pub(crate) fn accept(&mut self, locale: &str, key: &str) -> Result<(), AppError> {
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
        self.save_if_enabled()
    }

    pub(crate) fn accept_all_on_review(&mut self, locale: &str) -> Result<usize, AppError> {
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

    pub(crate) fn reject(&mut self, locale: &str, key: &str) -> Result<(), AppError> {
        let file = self
            .targets
            .get_mut(locale)
            .ok_or_else(|| AppError::Validation(format!("unknown locale {locale}")))?;
        let entry = file
            .entries
            .get_mut(key)
            .ok_or_else(|| AppError::Validation(format!("unknown key {key}")))?;
        entry.status = WAITING.into();
        self.save_if_enabled()
    }

    pub(crate) fn set_translation(
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
        self.save_if_enabled()
    }

    pub(crate) fn add_key(
        &mut self,
        key: &str,
        text: String,
        entry_type: String,
        story_id: Option<String>,
        owner_id: Option<String>,
    ) -> Result<(), AppError> {
        self.add_key_in_memory(key, text, entry_type, story_id, owner_id)?;
        self.save_if_enabled()
    }

    pub(crate) fn add_key_in_memory(
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
}
