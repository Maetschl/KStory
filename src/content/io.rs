use super::migration::{canonicalize_story_nodes, project_connections_to_legacy_fields};
use super::*;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DialogueFile {
    #[serde(default = "default_content_schema_version")]
    schema_version: u32,
    #[serde(default)]
    dialogues: BTreeMap<String, DialogueDefinition>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoryFile {
    #[serde(default = "default_content_schema_version")]
    schema_version: u32,
    #[serde(default)]
    stories: BTreeMap<String, StoryDefinition>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct QuestFile {
    #[serde(default = "default_content_schema_version")]
    schema_version: u32,
    #[serde(default)]
    quests: BTreeMap<String, QuestDefinition>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ItemFile {
    #[serde(default = "default_content_schema_version")]
    schema_version: u32,
    #[serde(default)]
    items: BTreeMap<String, ItemDefinition>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct EntityFile {
    #[serde(default = "default_content_schema_version")]
    schema_version: u32,
    #[serde(default)]
    entities: BTreeMap<String, EntityDefinition>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CallbackFile {
    #[serde(default = "default_content_schema_version")]
    schema_version: u32,
    #[serde(default)]
    callbacks: BTreeMap<String, CallbackDefinition>,
}

fn default_content_schema_version() -> u32 {
    CONTENT_SCHEMA_VERSION
}

impl ContentCatalog {
    pub fn load_files(paths: &ContentPaths) -> Result<Self, String> {
        let stories_file: Option<StoryFile> = read_optional(paths.stories.clone())?;
        let dialogues_file: Option<DialogueFile> = read_optional(paths.dialogues.clone())?;
        let quests_file: Option<QuestFile> = read_optional(paths.quests.clone())?;
        let items_file: Option<ItemFile> = read_optional(paths.items.clone())?;
        let entities_file: Option<EntityFile> = read_optional(paths.entities.clone())?;
        let callbacks_file: Option<CallbackFile> = read_optional(paths.callbacks.clone())?;

        for (name, version) in [
            (
                "stories.json",
                stories_file.as_ref().map(|file| file.schema_version),
            ),
            (
                "dialogues.json",
                dialogues_file.as_ref().map(|file| file.schema_version),
            ),
            (
                "quests.json",
                quests_file.as_ref().map(|file| file.schema_version),
            ),
            (
                "items.json",
                items_file.as_ref().map(|file| file.schema_version),
            ),
            (
                "entities.json",
                entities_file.as_ref().map(|file| file.schema_version),
            ),
            (
                "callbacks.json",
                callbacks_file.as_ref().map(|file| file.schema_version),
            ),
        ] {
            if let Some(version) = version {
                if version != CONTENT_SCHEMA_VERSION {
                    return Err(format!(
                        "{name} uses schema version {version}; expected schema version {CONTENT_SCHEMA_VERSION}"
                    ));
                }
            }
        }

        let mut catalog = Self {
            schema_version: CONTENT_SCHEMA_VERSION,
            stories: stories_file.map(|file| file.stories).unwrap_or_default(),
            dialogues: dialogues_file
                .map(|file| file.dialogues)
                .unwrap_or_default(),
            quests: quests_file.map(|file| file.quests).unwrap_or_default(),
            items: items_file.map(|file| file.items).unwrap_or_default(),
            entities: entities_file.map(|file| file.entities).unwrap_or_default(),
            callbacks: callbacks_file
                .map(|file| file.callbacks)
                .unwrap_or_default(),
        };
        catalog.normalize_legacy_line_ids();
        catalog.normalize_legacy_connections();
        catalog.normalize_legacy_payloads()?;
        Ok(catalog)
    }

    pub fn save(&self, root: impl AsRef<Path>) -> Result<(), String> {
        self.save_files(&ContentPaths::legacy(root))
    }

    pub fn save_files(&self, paths: &ContentPaths) -> Result<(), String> {
        let mut stories = self.stories.clone();
        for story in stories.values_mut() {
            project_connections_to_legacy_fields(story);
            canonicalize_story_nodes(story);
        }
        write_json(
            paths.stories.clone(),
            &StoryFile {
                schema_version: CONTENT_SCHEMA_VERSION,
                stories,
            },
        )?;
        write_json(
            paths.dialogues.clone(),
            &DialogueFile {
                schema_version: CONTENT_SCHEMA_VERSION,
                dialogues: self.dialogues.clone(),
            },
        )?;
        write_json(
            paths.quests.clone(),
            &QuestFile {
                schema_version: CONTENT_SCHEMA_VERSION,
                quests: self.quests.clone(),
            },
        )?;
        write_json(
            paths.items.clone(),
            &ItemFile {
                schema_version: CONTENT_SCHEMA_VERSION,
                items: self.items.clone(),
            },
        )?;
        write_json(
            paths.entities.clone(),
            &EntityFile {
                schema_version: CONTENT_SCHEMA_VERSION,
                entities: self.entities.clone(),
            },
        )?;
        write_json(
            paths.callbacks.clone(),
            &CallbackFile {
                schema_version: CONTENT_SCHEMA_VERSION,
                callbacks: self.callbacks.clone(),
            },
        )?;
        Ok(())
    }

    pub fn save_runtime_at(&self, root: impl AsRef<Path>) -> Result<(), String> {
        let runtime = root.as_ref().join("content");
        fs::create_dir_all(&runtime).map_err(|error| error.to_string())?;
        let mut stories = self.stories.clone();
        for story in stories.values_mut() {
            project_connections_to_legacy_fields(story);
            canonicalize_story_nodes(story);
        }
        write_json(
            runtime.join("stories.json"),
            &StoryFile {
                schema_version: CONTENT_SCHEMA_VERSION,
                stories,
            },
        )?;
        write_json(runtime.join("dialogues.json"), &self.dialogues)?;
        write_json(runtime.join("quests.json"), &self.quests)?;
        write_json(runtime.join("items.json"), &self.items)?;
        write_json(runtime.join("entities.json"), &self.entities)?;
        Ok(())
    }
}
fn read_optional<T: for<'de> Deserialize<'de>>(path: PathBuf) -> Result<Option<T>, String> {
    if !path.exists() {
        return Ok(None);
    }
    let text = fs::read_to_string(&path).map_err(|error| format!("{}: {error}", path.display()))?;
    serde_json::from_str(&text)
        .map(Some)
        .map_err(|error| format!("{}: {error}", path.display()))
}

fn write_json<T: Serialize>(path: PathBuf, value: &T) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| format!("{}: {error}", parent.display()))?;
    }
    let text = serde_json::to_string_pretty(value).map_err(|error| error.to_string())?;
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
            Err(error) => return Err(format!("{}: {error}", path.display())),
        }
    }
    let temp_path = temp_path
        .ok_or_else(|| format!("could not allocate temporary file for {}", path.display()))?;
    let mut temp_file = temp_file.expect("temporary file is present when its path is present");
    let write_result = (|| -> Result<(), String> {
        temp_file
            .write_all(format!("{text}\n").as_bytes())
            .map_err(|error| format!("{}: {error}", path.display()))?;
        temp_file
            .sync_all()
            .map_err(|error| format!("{}: {error}", path.display()))?;
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
        fs::remove_file(&path).map_err(|error| format!("{}: {error}", path.display()))?;
    }
    if let Err(error) = fs::rename(&temp_path, &path) {
        let _ = fs::remove_file(&temp_path);
        return Err(format!("{}: {error}", path.display()));
    }
    Ok(())
}
