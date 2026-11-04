use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[cfg(test)]
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

mod io;
mod layout;
mod migration;
mod model;
mod validation;

#[cfg(feature = "legacy-egui")]
pub use layout::node_type_color;
pub use layout::{arrange_story_top_down, outgoing_node_ids};
pub use model::*;

/// Version 1 is the first and only canonical content contract. Keep this
/// stable until the project explicitly introduces a new schema migration.
pub const CONTENT_SCHEMA_VERSION: u32 = 1;

pub const NODE_WIDTH: f32 = 220.0;
pub const NODE_HEIGHT: f32 = 92.0;
pub const LAYOUT_HORIZONTAL_GAP: f32 = 96.0;
pub const LAYOUT_VERTICAL_GAP: f32 = 84.0;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentPaths {
    pub stories: PathBuf,
    pub dialogues: PathBuf,
    pub quests: PathBuf,
    pub items: PathBuf,
    pub entities: PathBuf,
    pub callbacks: PathBuf,
}

impl ContentPaths {
    pub fn legacy(root: impl AsRef<Path>) -> Self {
        let root = root.as_ref();
        Self {
            stories: root.join("stories.json"),
            dialogues: root.join("dialogues.json"),
            quests: root.join("quests.json"),
            items: root.join("items.json"),
            entities: root.join("entities.json"),
            callbacks: root.join("callbacks.json"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StoryLayoutPosition {
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StoryLayoutIssues {
    pub orphaned_nodes: Vec<String>,
    pub cycle_edges: Vec<(String, String)>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConnectionDefinition {
    pub id: String,
    pub from_node_id: String,
    pub to_node_id: String,
    #[serde(default = "default_output_id")]
    pub output_id: String,
    #[serde(default = "default_connection_kind")]
    pub connection_kind: String,
    #[serde(default)]
    pub label_key: Option<String>,
    #[serde(default)]
    pub condition: Option<ConditionDefinition>,
}

fn default_output_id() -> String {
    "default".into()
}

fn default_connection_kind() -> String {
    "next".into()
}

const NODE_TYPES: &[&str] = &[
    "dialogue",
    "show_item",
    "show_icon",
    "choice",
    "condition",
    "action",
    "quest_start",
    "quest_update",
    "scene_transition",
    "event",
    "end",
];

const CONDITION_TYPES: &[&str] = &[
    "always",
    "has_item",
    "item_count",
    "quest_status",
    "flag",
    "event_happened",
    "entity_state",
    "callback",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentCatalog {
    pub schema_version: u32,
    #[serde(default)]
    pub stories: BTreeMap<String, StoryDefinition>,
    #[serde(default)]
    pub dialogues: BTreeMap<String, DialogueDefinition>,
    #[serde(default)]
    pub quests: BTreeMap<String, QuestDefinition>,
    #[serde(default)]
    pub items: BTreeMap<String, ItemDefinition>,
    #[serde(default)]
    pub entities: BTreeMap<String, EntityDefinition>,
    #[serde(default)]
    pub callbacks: BTreeMap<String, CallbackDefinition>,
}

impl Default for ContentCatalog {
    fn default() -> Self {
        Self {
            schema_version: CONTENT_SCHEMA_VERSION,
            stories: BTreeMap::new(),
            dialogues: BTreeMap::new(),
            quests: BTreeMap::new(),
            items: BTreeMap::new(),
            entities: BTreeMap::new(),
            callbacks: BTreeMap::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn story(nodes: &[(&str, Vec<&str>)]) -> StoryDefinition {
        StoryDefinition {
            name_key: String::new(),
            description_key: String::new(),
            owner_id: String::new(),
            entry_node_id: "start".into(),
            chapters: Vec::new(),
            nodes: nodes
                .iter()
                .map(|(id, next)| {
                    (
                        (*id).into(),
                        StoryNode {
                            node_type: "dialogue".into(),
                            name_key: String::new(),
                            payload: None,
                            chapter_id: None,
                            dialogue_id: None,
                            dialogue_line_id: None,
                            quest_id: None,
                            scene_id: None,
                            event_id: None,
                            action: None,
                            condition: None,
                            choices: Vec::new(),
                            next: next.iter().map(|value| (*value).into()).collect(),
                            position: [0.0, 0.0],
                        },
                    )
                })
                .collect(),
            connections: BTreeMap::new(),
        }
    }

    #[test]
    fn legacy_lines_receive_stable_ids_from_keys() {
        let mut catalog = ContentCatalog::default();
        catalog.dialogues.insert(
            "intro".into(),
            DialogueDefinition {
                name_key: String::new(),
                story_id: None,
                chapter_id: None,
                owner_id: None,
                lines: vec![DialogueLineDefinition {
                    id: String::new(),
                    kind: "text".into(),
                    key: Some("dialog.intro.hello".into()),
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
        catalog.normalize_legacy_line_ids();
        assert_eq!(catalog.dialogues["intro"].lines[0].id, "dialog.intro.hello");
    }

    #[test]
    fn legacy_next_and_choices_are_migrated_to_stable_connections() {
        let mut catalog = ContentCatalog::default();
        let mut story = story(&[("start", vec!["end"]), ("end", vec![])]);
        story
            .nodes
            .get_mut("start")
            .unwrap()
            .choices
            .push(ChoiceDefinition {
                id: "choice_a".into(),
                label_key: "dialog.choice.a".into(),
                next: "end".into(),
                condition: None,
            });
        catalog.stories.insert("story".into(), story);
        catalog.normalize_legacy_connections();
        let migrated = &catalog.stories["story"].connections;
        assert_eq!(migrated.len(), 2);
        assert!(migrated.values().any(|edge| edge.output_id == "default"));
        assert!(migrated.values().any(|edge| edge.output_id == "choice_a"));
        let mut projected = catalog.stories["story"].clone();
        super::migration::project_connections_to_legacy_fields(&mut projected);
        assert_eq!(projected.nodes["start"].next, vec!["end"]);
        assert_eq!(projected.nodes["start"].choices.len(), 1);
    }

    #[test]
    fn invalid_callback_blocks_catalog_validation() {
        let mut catalog = ContentCatalog::default();
        catalog.stories.insert(
            "story".into(),
            StoryDefinition {
                name_key: String::new(),
                description_key: String::new(),
                owner_id: String::new(),
                entry_node_id: "start".into(),
                chapters: Vec::new(),
                nodes: BTreeMap::from([(
                    "start".into(),
                    StoryNode {
                        node_type: "action".into(),
                        name_key: String::new(),
                        payload: Some(NodePayload::Action(ActionDefinition::Callback {
                            callback_id: "missing".into(),
                            params: BTreeMap::new(),
                        })),
                        chapter_id: None,
                        dialogue_id: None,
                        dialogue_line_id: None,
                        quest_id: None,
                        scene_id: None,
                        event_id: None,
                        action: Some(ActionDefinition::Callback {
                            callback_id: "missing".into(),
                            params: BTreeMap::new(),
                        }),
                        condition: None,
                        choices: Vec::new(),
                        next: Vec::new(),
                        position: [0.0, 0.0],
                    },
                )]),
                connections: BTreeMap::new(),
            },
        );
        let errors = catalog.validate(&BTreeSet::new());
        assert!(errors
            .iter()
            .any(|error| error.contains("missing callback")));
    }

    #[test]
    fn validation_reports_nodes_not_reachable_from_entry() {
        let mut catalog = ContentCatalog::default();
        catalog.stories.insert(
            "story".into(),
            story(&[
                ("start", vec![]),
                ("incoming", vec!["bridge"]),
                ("bridge", vec!["orphan"]),
                ("orphan", vec![]),
            ]),
        );

        let errors = catalog.validate(&BTreeSet::new());
        assert!(errors
            .iter()
            .any(|error| error.contains("node incoming is unreachable")));
        assert!(errors
            .iter()
            .any(|error| error.contains("node bridge is unreachable")));
        assert!(errors
            .iter()
            .any(|error| error.contains("node orphan is unreachable")));
    }

    #[test]
    fn validation_allows_intentional_cycles() {
        let mut catalog = ContentCatalog::default();
        catalog.stories.insert(
            "story".into(),
            story(&[("start", vec!["loop"]), ("loop", vec!["loop"])]),
        );

        assert!(catalog.validate(&BTreeSet::new()).is_empty());
    }

    #[test]
    fn top_down_layout_places_linear_nodes_on_increasing_levels() {
        let story = story(&[
            ("start", vec!["middle"]),
            ("middle", vec!["end"]),
            ("end", vec![]),
        ]);
        let (positions, issues) = arrange_story_top_down(&story);
        assert!(issues.orphaned_nodes.is_empty());
        assert!(positions["start"].y < positions["middle"].y);
        assert!(positions["middle"].y < positions["end"].y);
    }

    #[test]
    fn top_down_layout_distributes_branches_stably() {
        let story = story(&[
            ("start", vec!["right", "left"]),
            ("left", vec![]),
            ("right", vec![]),
        ]);
        let (positions, _) = arrange_story_top_down(&story);
        assert_eq!(positions["left"].y, positions["right"].y);
        assert!(positions["left"].x < positions["right"].x);
        assert_eq!(positions, arrange_story_top_down(&story).0);
    }

    #[test]
    fn top_down_layout_reports_orphans_and_cycles() {
        let story = story(&[
            ("start", vec!["loop"]),
            ("loop", vec!["loop"]),
            ("orphan", vec![]),
        ]);
        let (_, issues) = arrange_story_top_down(&story);
        assert_eq!(issues.orphaned_nodes, vec!["orphan"]);
        assert_eq!(issues.cycle_edges, vec![("loop".into(), "loop".into())]);
    }

    #[cfg(feature = "legacy-egui")]
    #[test]
    fn node_colors_are_semantic() {
        assert_ne!(node_type_color("dialogue"), node_type_color("choice"));
        assert_eq!(node_type_color("end"), [99, 111, 125]);
    }

    #[test]
    fn typed_actions_reject_mixed_camera_and_item_parameters() {
        let invalid = serde_json::from_value::<ActionDefinition>(serde_json::json!({
            "action_type": "camera_shake",
            "params": { "item_id": "HAMMER" }
        }));
        assert!(invalid.is_err());
        let start = serde_json::from_value::<ActionDefinition>(serde_json::json!({
            "action_type": "camera_shake",
            "params": { "phase": "start" }
        }))
        .unwrap();
        assert!(matches!(start, ActionDefinition::CameraShakeStart));
    }

    #[test]
    fn typed_node_payloads_keep_display_commands_out_of_dialogues() {
        let node = StoryNode {
            node_type: "show_item".into(),
            name_key: String::new(),
            payload: Some(NodePayload::ShowItem {
                item_id: "HAMMER".into(),
            }),
            chapter_id: None,
            dialogue_id: None,
            dialogue_line_id: None,
            quest_id: None,
            scene_id: None,
            event_id: None,
            action: None,
            condition: None,
            choices: Vec::new(),
            next: Vec::new(),
            position: [0.0, 0.0],
        };
        let json = serde_json::to_value(&node).unwrap();
        assert_eq!(json["payload"]["type"], "show_item");
        assert!(json.get("command").is_none());
    }
}
