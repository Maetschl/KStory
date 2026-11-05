use crate::content;
use clap::{Parser, Subcommand};
#[path = "dioxus_ui/mod.rs"]
mod dioxus_ui;
#[path = "store/mod.rs"]
mod store;

#[cfg(test)]
use crate::project;
#[cfg(test)]
use std::collections::{BTreeMap, BTreeSet};
#[cfg(test)]
use std::fs;
use std::path::PathBuf;
pub(crate) use store::*;
use thiserror::Error;

const WAITING: &str = "waiting_for_translation";
const ON_REVIEW: &str = "on_review";
const ACCEPTED: &str = "accepted";
const CATALOG_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Error)]
pub(crate) enum AppError {
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

pub(crate) fn run() -> Result<(), AppError> {
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
#[allow(
    dead_code,
    reason = "store filtering preserves all supported sort modes"
)]
pub(crate) enum SortMode {
    #[default]
    Key,
    Status,
    Story,
    Owner,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct FilterState {
    search: String,
    status: String,
    entry_type: String,
    story: String,
    quest: String,
    owner: String,
    only_orphaned: bool,
    sort_mode: SortMode,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct EntryViewModel {
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

fn run_gui(root: PathBuf) -> Result<(), AppError> {
    dioxus_ui::run(root)
}

#[cfg(test)]
mod tests {
    use super::*;
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
    fn rename_key_updates_typed_content_and_translations() {
        let (_directory, mut store) = test_store();
        store.content.dialogues.insert(
            "intro".into(),
            content::DialogueDefinition {
                name_key: String::new(),
                story_id: None,
                chapter_id: None,
                owner_id: None,
                lines: vec![content::DialogueLineDefinition {
                    id: "line".into(),
                    kind: "text".into(),
                    key: Some("dialog.test".into()),
                    command: None,
                    speaker: None,
                    variables: Vec::new(),
                    on_enter: None,
                    on_exit: None,
                }],
                on_enter: Vec::new(),
                on_exit: Vec::new(),
            },
        );

        store.rename_key("dialog.test", "dialog.renamed").unwrap();

        assert!(store.source.entries.contains_key("dialog.renamed"));
        assert!(!store.source.entries.contains_key("dialog.test"));
        assert!(store.targets["es"].entries.contains_key("dialog.renamed"));
        assert_eq!(
            store.content.dialogues["intro"].lines[0].key.as_deref(),
            Some("dialog.renamed")
        );
    }

    #[test]
    fn rename_key_restores_memory_when_validation_fails() {
        let (_directory, mut store) = test_store();
        store.content.dialogues.insert(
            "intro".into(),
            content::DialogueDefinition {
                name_key: String::new(),
                story_id: None,
                chapter_id: None,
                owner_id: None,
                lines: vec![
                    content::DialogueLineDefinition {
                        id: "line".into(),
                        kind: "text".into(),
                        key: Some("dialog.test".into()),
                        command: None,
                        speaker: None,
                        variables: Vec::new(),
                        on_enter: None,
                        on_exit: None,
                    },
                    content::DialogueLineDefinition {
                        id: "missing".into(),
                        kind: "text".into(),
                        key: Some("dialog.missing".into()),
                        command: None,
                        speaker: None,
                        variables: Vec::new(),
                        on_enter: None,
                        on_exit: None,
                    },
                ],
                on_enter: Vec::new(),
                on_exit: Vec::new(),
            },
        );

        assert!(store.rename_key("dialog.test", "dialog.renamed").is_err());
        assert!(store.source.entries.contains_key("dialog.test"));
        assert!(!store.source.entries.contains_key("dialog.renamed"));
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
            persist_mutations: true,
        };
        let filters = FilterState {
            story: "even".into(),
            ..Default::default()
        };
        assert_eq!(store.filtered_keys("es", &filters).len(), 5_000);
    }

    #[test]
    fn editor_session_undoes_and_redoes_a_compound_story_edit() {
        let (_directory, store) = test_store();
        let mut session = EditorSession::new(store);
        let created = session.create_story().unwrap();
        assert!(session.content.stories.contains_key(&created.story_id));

        session.undo().unwrap();
        assert!(session.content.stories.is_empty());
        assert!(!session.can_undo());
        assert!(session.can_redo());

        session.redo().unwrap();
        assert!(session.content.stories.contains_key(&created.story_id));
    }

    #[test]
    fn editor_session_undoes_and_redoes_translation_edits() {
        let (_directory, store) = test_store();
        let mut session = EditorSession::new(store);
        session
            .save_translation("es", "dialog.test", "Hola de nuevo {name}".into())
            .unwrap();
        assert_eq!(
            session.targets["es"].entries["dialog.test"].text,
            "Hola de nuevo {name}"
        );

        session.undo().unwrap();
        assert_eq!(
            session.targets["es"].entries["dialog.test"].text,
            "Hola {name}"
        );
        session.redo().unwrap();
        assert_eq!(
            session.targets["es"].entries["dialog.test"].text,
            "Hola de nuevo {name}"
        );
    }

    #[test]
    fn failed_session_mutations_are_rolled_back_without_history() {
        let (_directory, store) = test_store();
        let mut session = EditorSession::new(store);
        let result = session.edit("Failed edit", None, |store| {
            store.catalog.source_locale = "fr".into();
            Err::<(), AppError>(AppError::Validation("expected failure".into()))
        });

        assert!(result.is_err());
        assert_eq!(session.catalog.source_locale, "en");
        assert!(!session.can_undo());
    }

    #[test]
    fn failed_save_keeps_session_dirty() {
        let (directory, store) = test_store();
        let mut session = EditorSession::new(store);
        session.paths.keys = directory.path().to_path_buf();
        let _ = session.edit("Edit before failed save", None, |store| {
            store.catalog.source_locale = "fr".into();
            Ok(())
        });

        assert!(session.is_dirty());
        assert!(session.save_explicit().is_err());
        assert!(session.is_dirty());
    }
}
