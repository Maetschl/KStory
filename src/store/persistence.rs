use super::*;

impl Store {
    pub(crate) fn load(root: impl AsRef<Path>) -> Result<Self, AppError> {
        let paths = project::ProjectPaths::from_input(root).map_err(AppError::Validation)?;
        Self::load_paths(paths)
    }

    pub(crate) fn load_paths(paths: project::ProjectPaths) -> Result<Self, AppError> {
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
            paths,
            catalog,
            source,
            targets,
            content,
            persist_mutations: true,
        })
    }

    pub(crate) fn save(&self) -> Result<(), AppError> {
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

    pub(crate) fn save_all(&self) -> Result<(), AppError> {
        self.save_content()?;
        self.save()
    }

    pub(crate) fn save_if_enabled(&self) -> Result<(), AppError> {
        if self.persist_mutations {
            self.save_all()
        } else {
            Ok(())
        }
    }

    pub(crate) fn save_content(&self) -> Result<(), AppError> {
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
    pub(crate) fn import_runtime_source_if_empty(&mut self) -> Result<(), AppError> {
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

    pub(crate) fn import_runtime_targets_if_empty(&mut self) -> Result<(), AppError> {
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
    pub(crate) fn export(&self) -> Result<(), AppError> {
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
