use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Component, Path, PathBuf};

pub const PROJECT_FORMAT_VERSION: u32 = 1;
pub const CONTENT_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectManifest {
    pub format_version: u32,
    pub content_schema_version: u32,
    pub project_id: String,
    pub paths: ProjectPathConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectPathConfig {
    pub keys: String,
    pub stories: String,
    pub dialogues: String,
    pub quests: String,
    pub items: String,
    pub entities: String,
    pub callbacks: String,
    pub locales: String,
    pub export: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectPaths {
    pub manifest_path: Option<PathBuf>,
    pub project_dir: PathBuf,
    pub project_id: String,
    pub keys: PathBuf,
    pub stories: PathBuf,
    pub dialogues: PathBuf,
    pub quests: PathBuf,
    pub items: PathBuf,
    pub entities: PathBuf,
    pub callbacks: PathBuf,
    pub locales: PathBuf,
    pub export: PathBuf,
}

impl ProjectPaths {
    pub fn from_input(input: impl AsRef<Path>) -> Result<Self, String> {
        let input = input.as_ref();
        if input.extension().and_then(|extension| extension.to_str()) == Some("kstory") {
            Self::from_manifest(input)
        } else {
            Self::legacy(input)
        }
    }

    pub fn from_manifest(path: impl AsRef<Path>) -> Result<Self, String> {
        let path = path.as_ref();
        let text = fs::read_to_string(path).map_err(|error| {
            format!(
                "unable to read project manifest {}: {error}",
                path.display()
            )
        })?;
        let manifest: ProjectManifest = serde_json::from_str(&text)
            .map_err(|error| format!("invalid project manifest {}: {error}", path.display()))?;
        if manifest.format_version != PROJECT_FORMAT_VERSION {
            return Err(format!(
                "project manifest uses format version {}; expected {}",
                manifest.format_version, PROJECT_FORMAT_VERSION
            ));
        }
        if manifest.content_schema_version != CONTENT_SCHEMA_VERSION {
            return Err(format!(
                "project manifest expects content schema {}; expected {}",
                manifest.content_schema_version, CONTENT_SCHEMA_VERSION
            ));
        }
        let project_dir = path
            .parent()
            .ok_or_else(|| {
                format!(
                    "project manifest has no parent directory: {}",
                    path.display()
                )
            })?
            .canonicalize()
            .map_err(|error| format!("unable to resolve project directory: {error}"))?;
        let resolve = |name: &str, relative: &str| -> Result<PathBuf, String> {
            let relative_path = Path::new(relative);
            validate_relative_path(name, relative_path)?;
            Ok(project_dir.join(relative_path))
        };
        Ok(Self {
            manifest_path: Some(path.to_path_buf()),
            project_dir: project_dir.clone(),
            project_id: manifest.project_id,
            keys: resolve("keys", &manifest.paths.keys)?,
            stories: resolve("stories", &manifest.paths.stories)?,
            dialogues: resolve("dialogues", &manifest.paths.dialogues)?,
            quests: resolve("quests", &manifest.paths.quests)?,
            items: resolve("items", &manifest.paths.items)?,
            entities: resolve("entities", &manifest.paths.entities)?,
            callbacks: resolve("callbacks", &manifest.paths.callbacks)?,
            locales: resolve("locales", &manifest.paths.locales)?,
            export: resolve("export", &manifest.paths.export)?,
        })
    }

    pub fn legacy(root: impl AsRef<Path>) -> Result<Self, String> {
        let root = root.as_ref();
        if !root.is_dir() {
            return Err(format!(
                "project input must be a .kstory file or an existing directory: {}",
                root.display()
            ));
        }
        let project_dir = root
            .canonicalize()
            .map_err(|error| format!("unable to resolve project directory: {error}"))?;
        Ok(Self {
            manifest_path: None,
            project_id: project_dir
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("project")
                .to_string(),
            keys: project_dir.join("keys.json"),
            stories: project_dir.join("stories.json"),
            dialogues: project_dir.join("dialogues.json"),
            quests: project_dir.join("quests.json"),
            items: project_dir.join("items.json"),
            entities: project_dir.join("entities.json"),
            callbacks: project_dir.join("callbacks.json"),
            locales: project_dir.join("locales"),
            export: project_dir.join("runtime"),
            project_dir,
        })
    }

    pub fn locale_file(&self, locale: &str) -> PathBuf {
        self.locales.join(format!("{locale}.json"))
    }
}

fn validate_relative_path(name: &str, path: &Path) -> Result<(), String> {
    if path.as_os_str().is_empty() {
        return Err(format!("project path '{name}' cannot be empty"));
    }
    if path.is_absolute() {
        return Err(format!("project path '{name}' must be relative"));
    }
    if path.components().any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    }) {
        return Err(format!(
            "project path '{name}' cannot escape the .kstory directory"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn manifest(root: &Path) -> PathBuf {
        let path = root.join("Game.kstory");
        fs::write(
            &path,
            serde_json::to_string_pretty(&ProjectManifest {
                format_version: 1,
                content_schema_version: 1,
                project_id: "game".into(),
                paths: ProjectPathConfig {
                    keys: "content/keys.json".into(),
                    stories: "content/stories.json".into(),
                    dialogues: "content/dialogues.json".into(),
                    quests: "content/quests.json".into(),
                    items: "content/items.json".into(),
                    entities: "content/entities.json".into(),
                    callbacks: "content/callbacks.json".into(),
                    locales: "content/locales".into(),
                    export: "build/kstory".into(),
                },
            })
            .unwrap(),
        )
        .unwrap();
        path
    }

    #[test]
    fn resolves_manifest_paths_relative_to_manifest_directory() {
        let directory = tempdir().unwrap();
        let paths = ProjectPaths::from_manifest(manifest(directory.path())).unwrap();
        let directory_path = directory.path().canonicalize().unwrap();
        assert_eq!(paths.project_id, "game");
        assert_eq!(paths.keys, directory_path.join("content/keys.json"));
        assert_eq!(
            paths.locale_file("es"),
            directory_path.join("content/locales/es.json")
        );
        assert_eq!(paths.export, directory_path.join("build/kstory"));
    }

    #[test]
    fn rejects_absolute_and_parent_paths() {
        let directory = tempdir().unwrap();
        let path = manifest(directory.path());
        let mut value: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        value["paths"]["keys"] = serde_json::Value::String("../keys.json".into());
        fs::write(&path, serde_json::to_string(&value).unwrap()).unwrap();
        let error = ProjectPaths::from_manifest(path).unwrap_err();
        assert!(error.contains("cannot escape"));
    }

    #[test]
    fn accepts_legacy_directory_for_migration_and_tests() {
        let directory = tempdir().unwrap();
        let paths = ProjectPaths::from_input(directory.path()).unwrap();
        let directory_path = directory.path().canonicalize().unwrap();
        assert!(paths.manifest_path.is_none());
        assert_eq!(paths.keys, directory_path.join("keys.json"));
    }
}
