use super::*;
use crate::history::{History, HistoryLimit};
use std::fs;
use std::ops::{Deref, DerefMut};
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub(crate) struct EditorSession {
    store: Store,
    history: History<StoreState>,
    preferences: AppPreferences,
    last_save_error: Option<String>,
    observed: StoreState,
}

impl EditorSession {
    pub(crate) fn new(mut store: Store) -> Self {
        let preferences = load_preferences();
        store.persist_mutations = false;
        let state = StoreState::from_store(&store);
        Self {
            store,
            history: History::new(state.clone(), preferences.history_limit()),
            preferences,
            last_save_error: None,
            observed: state,
        }
    }

    pub(crate) fn store_snapshot(&self) -> Store {
        self.store.clone()
    }

    pub(crate) fn preferences(&self) -> &AppPreferences {
        &self.preferences
    }

    pub(crate) fn is_dirty(&self) -> bool {
        self.history.is_dirty(&StoreState::from_store(&self.store))
    }

    pub(crate) fn can_undo(&self) -> bool {
        self.history.can_undo()
    }

    pub(crate) fn can_redo(&self) -> bool {
        self.history.can_redo()
    }

    pub(crate) fn undo_label(&self) -> Option<&str> {
        self.history.undo_label()
    }

    pub(crate) fn redo_label(&self) -> Option<&str> {
        self.history.redo_label()
    }

    #[cfg(feature = "legacy-egui")]
    pub(crate) fn last_save_error(&self) -> Option<&str> {
        self.last_save_error.as_deref()
    }

    pub(crate) fn edit<R>(
        &mut self,
        label: impl Into<String>,
        coalesce_key: Option<String>,
        mutation: impl FnOnce(&mut Store) -> Result<R, AppError>,
    ) -> Result<R, AppError> {
        let before = StoreState::from_store(&self.store);
        let result = match mutation(&mut self.store) {
            Ok(result) => result,
            Err(error) => {
                before.clone().apply_to(&mut self.store);
                return Err(error);
            }
        };
        let after = StoreState::from_store(&self.store);
        if before != after {
            self.history
                .record(label, before, after.clone(), coalesce_key);
            self.observed = after.clone();
            if self.preferences.auto_save {
                self.persist_after_edit(&after)?;
            }
        }
        Ok(result)
    }

    pub(crate) fn undo(&mut self) -> Result<Option<String>, AppError> {
        let Some((label, state)) = self.history.undo() else {
            return Ok(None);
        };
        state.apply_to(&mut self.store);
        self.observed = StoreState::from_store(&self.store);
        if self.preferences.auto_save {
            let current = self.observed.clone();
            self.persist_after_edit(&current)?;
        }
        Ok(Some(label))
    }

    pub(crate) fn redo(&mut self) -> Result<Option<String>, AppError> {
        let Some((label, state)) = self.history.redo() else {
            return Ok(None);
        };
        state.apply_to(&mut self.store);
        self.observed = StoreState::from_store(&self.store);
        if self.preferences.auto_save {
            let current = self.observed.clone();
            self.persist_after_edit(&current)?;
        }
        Ok(Some(label))
    }

    pub(crate) fn save_explicit(&mut self) -> Result<(), AppError> {
        self.store.save_all()?;
        let current = StoreState::from_store(&self.store);
        self.history.mark_saved(&current);
        self.observed = current;
        self.last_save_error = None;
        Ok(())
    }

    pub(crate) fn sync_and_reset(&mut self) -> Result<SyncReport, AppError> {
        let report = self.store.sync()?;
        let current = StoreState::from_store(&self.store);
        self.history.reset(current.clone());
        self.observed = current;
        self.last_save_error = None;
        Ok(report)
    }

    pub(crate) fn update_preferences(
        &mut self,
        mut preferences: AppPreferences,
    ) -> Result<(), AppError> {
        preferences.normalize();
        let was_auto_save = self.preferences.auto_save;
        self.preferences = preferences;
        self.history.set_limit(self.preferences.history_limit());
        save_preferences(&self.preferences)?;
        if !was_auto_save && self.preferences.auto_save && self.is_dirty() {
            self.save_explicit()?;
        }
        Ok(())
    }

    #[cfg(feature = "legacy-egui")]
    pub(crate) fn observe_external_mutations(
        &mut self,
        label: impl Into<String>,
        coalesce_key: Option<String>,
    ) -> bool {
        let current = StoreState::from_store(&self.store);
        if current == self.observed {
            return false;
        }
        self.history
            .record(label, self.observed.clone(), current.clone(), coalesce_key);
        self.observed = current.clone();
        if self.preferences.auto_save {
            let _ = self.persist_after_edit(&current);
        }
        true
    }

    fn persist_after_edit(&mut self, state: &StoreState) -> Result<(), AppError> {
        match self.store.save_all() {
            Ok(()) => {
                self.history.mark_saved(state);
                self.last_save_error = None;
                Ok(())
            }
            Err(error) => {
                self.last_save_error = Some(error.to_string());
                Err(error)
            }
        }
    }

    pub(crate) fn create_story(&mut self) -> Result<CreatedStory, AppError> {
        self.edit("Create story", None, |store| store.create_story())
    }

    pub(crate) fn create_node(
        &mut self,
        story_id: &str,
        node_type: &str,
        speaker: &str,
        text: &str,
        parameter: Option<&str>,
        connection: Option<(String, String, String)>,
    ) -> Result<CreatedNode, AppError> {
        self.edit("Create node", None, |store| {
            store.create_node(story_id, node_type, speaker, text, parameter, connection)
        })
    }

    pub(crate) fn connect_nodes(
        &mut self,
        story_id: &str,
        request: ConnectionRequest,
    ) -> Result<String, AppError> {
        self.edit("Connect nodes", None, |store| {
            store.connect_nodes(story_id, request)
        })
    }

    pub(crate) fn update_connection(
        &mut self,
        story_id: &str,
        connection_id: &str,
        request: ConnectionRequest,
    ) -> Result<(), AppError> {
        self.edit("Update connection", None, |store| {
            store.update_connection(story_id, connection_id, request)
        })
    }

    pub(crate) fn delete_connection(
        &mut self,
        story_id: &str,
        connection_id: &str,
    ) -> Result<(), AppError> {
        self.edit("Delete connection", None, |store| {
            store.delete_connection(story_id, connection_id)
        })
    }

    #[allow(
        dead_code,
        reason = "content deletion gateway for the next editor controls"
    )]
    pub(crate) fn delete_node(&mut self, story_id: &str, node_id: &str) -> Result<(), AppError> {
        self.edit("Delete node", None, |store| {
            store.delete_node(story_id, node_id)
        })
    }

    pub(crate) fn update_node(
        &mut self,
        story_id: &str,
        node_id: &str,
        name_key: String,
        node_type: String,
    ) -> Result<(), AppError> {
        self.edit(
            "Edit node",
            Some(format!("node:{story_id}:{node_id}")),
            |store| store.update_node(story_id, node_id, name_key, node_type),
        )
    }

    #[allow(
        dead_code,
        reason = "key registry gateway for the next editor controls"
    )]
    pub(crate) fn rename_key(&mut self, old_key: &str, new_key: &str) -> Result<(), AppError> {
        self.edit("Rename key", None, |store| {
            store.rename_key(old_key, new_key)
        })
    }

    pub(crate) fn save_translation(
        &mut self,
        locale: &str,
        key: &str,
        text: String,
    ) -> Result<(), AppError> {
        self.edit("Save translation", None, |store| {
            store.save_translation(locale, key, text)
        })
    }

    pub(crate) fn mark_on_review(&mut self, locale: &str, key: &str) -> Result<(), AppError> {
        self.edit("Mark translation on review", None, |store| {
            store.mark_on_review(locale, key)
        })
    }

    pub(crate) fn accept(&mut self, locale: &str, key: &str) -> Result<(), AppError> {
        self.edit("Accept translation", None, |store| {
            store.accept(locale, key)
        })
    }

    #[allow(
        dead_code,
        reason = "bulk translation gateway for the next editor controls"
    )]
    pub(crate) fn accept_all_on_review(&mut self, locale: &str) -> Result<usize, AppError> {
        self.edit("Accept translations", None, |store| {
            store.accept_all_on_review(locale)
        })
    }

    #[allow(
        dead_code,
        reason = "translation rejection gateway for the next editor controls"
    )]
    pub(crate) fn reject(&mut self, locale: &str, key: &str) -> Result<(), AppError> {
        self.edit("Reject translation", None, |store| {
            store.reject(locale, key)
        })
    }

    #[allow(
        dead_code,
        reason = "translation reset gateway for the next editor controls"
    )]
    pub(crate) fn reset_translation(&mut self, locale: &str, key: &str) -> Result<(), AppError> {
        self.edit("Reset translation", None, |store| {
            store.reset_translation(locale, key)
        })
    }

    #[allow(
        dead_code,
        reason = "key registry gateway for the next editor controls"
    )]
    pub(crate) fn add_key(
        &mut self,
        key: &str,
        text: String,
        entry_type: String,
        story_id: Option<String>,
        owner_id: Option<String>,
    ) -> Result<(), AppError> {
        self.edit("Add key", None, |store| {
            store.add_key(key, text, entry_type, story_id, owner_id)
        })
    }
}

impl Deref for EditorSession {
    type Target = Store;

    fn deref(&self) -> &Self::Target {
        &self.store
    }
}

impl DerefMut for EditorSession {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.store
    }
}

impl AppPreferences {
    fn normalize(&mut self) {
        self.history_limit = self.history_limit.max(1);
    }

    fn history_limit(&self) -> HistoryLimit {
        if self.unlimited_history {
            HistoryLimit::Unlimited
        } else {
            HistoryLimit::Bounded(self.history_limit.max(1))
        }
    }
}

fn preferences_path() -> Option<PathBuf> {
    dirs::config_dir().map(|directory| directory.join("KStory").join("settings.json"))
}

fn load_preferences() -> AppPreferences {
    let Some(path) = preferences_path() else {
        return AppPreferences::default();
    };
    fs::read_to_string(path)
        .ok()
        .map(|text| parse_preferences(&text))
        .unwrap_or_default()
}

fn parse_preferences(text: &str) -> AppPreferences {
    serde_json::from_str(text)
        .map(|mut preferences: AppPreferences| {
            preferences.normalize();
            preferences
        })
        .unwrap_or_default()
}

fn save_preferences(preferences: &AppPreferences) -> Result<(), AppError> {
    let Some(path) = preferences_path() else {
        return Ok(());
    };
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let text = serde_json::to_string_pretty(preferences)
        .map_err(|error| AppError::Validation(error.to_string()))?;
    fs::write(path, format!("{text}\n"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preferences_use_defaults_for_missing_or_invalid_values() {
        assert_eq!(parse_preferences("{}"), AppPreferences::default());
        assert_eq!(parse_preferences(r#"{"history_limit":0}"#).history_limit, 1);
        assert_eq!(
            parse_preferences(r#"{"history_limit":"large"}"#),
            AppPreferences::default()
        );
    }
}
