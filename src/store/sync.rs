use super::*;

impl Store {
    pub(crate) fn sync(&mut self) -> Result<SyncReport, AppError> {
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
    pub(crate) fn content_metadata(&self) -> Result<BTreeMap<String, KeyEntry>, AppError> {
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
    pub(crate) fn validate(&self) -> Result<Vec<String>, AppError> {
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
}
