use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

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

impl ContentCatalog {
    pub fn load(root: impl AsRef<Path>) -> Result<Self, String> {
        Self::load_files(&ContentPaths::legacy(root))
    }

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

    pub fn save_runtime(&self, root: impl AsRef<Path>) -> Result<(), String> {
        self.save_runtime_at(root.as_ref().join("runtime"))
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

    pub fn validate(&self, source_keys: &BTreeSet<String>) -> Vec<String> {
        let mut errors = Vec::new();
        if self.schema_version != CONTENT_SCHEMA_VERSION {
            errors.push(format!(
                "content schema version {} is unsupported",
                self.schema_version
            ));
        }

        for (story_id, story) in &self.stories {
            if story.entry_node_id.is_empty() {
                errors.push(format!("story {story_id} has no entry node"));
            } else if !story.nodes.contains_key(&story.entry_node_id) {
                errors.push(format!(
                    "story {story_id} entry node {} does not exist",
                    story.entry_node_id
                ));
            }
            let mut incoming = BTreeSet::new();
            for (node_id, node) in &story.nodes {
                if !NODE_TYPES.contains(&node.node_type.as_str()) {
                    errors.push(format!(
                        "story {story_id} node {node_id} has unknown type {}",
                        node.node_type
                    ));
                }
                if let Some(payload) = &node.payload {
                    validate_node_payload(
                        &mut errors,
                        &format!("story {story_id} node {node_id}"),
                        &node.node_type,
                        payload,
                        self,
                    );
                }
                for next in &node.next {
                    incoming.insert(next.clone());
                    if !story.nodes.contains_key(next) {
                        errors.push(format!(
                            "story {story_id} node {node_id} points to missing node {next}"
                        ));
                    }
                }
                for choice in &node.choices {
                    incoming.insert(choice.next.clone());
                    if !story.nodes.contains_key(&choice.next) {
                        errors.push(format!(
                            "story {story_id} choice {} points to missing node {}",
                            choice.id, choice.next
                        ));
                    }
                    if let Some(condition) = &choice.condition {
                        validate_condition(
                            &mut errors,
                            &format!("story {story_id} choice {}", choice.id),
                            condition,
                            &self.callbacks,
                        );
                    }
                }
                if let Some(dialogue_id) = &node.dialogue_id {
                    if !self.dialogues.contains_key(dialogue_id) {
                        errors.push(format!("story {story_id} node {node_id} references missing dialogue {dialogue_id}"));
                    } else if let Some(line_id) = &node.dialogue_line_id {
                        let exists = self.dialogues[dialogue_id]
                            .lines
                            .iter()
                            .any(|line| line.id == *line_id);
                        if !exists {
                            errors.push(format!(
                                "story {story_id} node {node_id} references missing dialogue line {dialogue_id}:{line_id}"
                            ));
                        }
                    }
                } else if node.dialogue_line_id.is_some() {
                    errors.push(format!(
                        "story {story_id} node {node_id} has a line id without a dialogue"
                    ));
                }
                if let Some(quest_id) = &node.quest_id {
                    if !self.quests.contains_key(quest_id) {
                        errors.push(format!(
                            "story {story_id} node {node_id} references missing quest {quest_id}"
                        ));
                    }
                }
                if let Some(action) = &node.action {
                    validate_action(
                        &mut errors,
                        &format!("story {story_id} node {node_id}"),
                        action,
                        self,
                    );
                }
                if let Some(condition) = &node.condition {
                    validate_condition(
                        &mut errors,
                        &format!("story {story_id} node {node_id}"),
                        condition,
                        &self.callbacks,
                    );
                }
            }
            let mut connection_slots = BTreeSet::new();
            for (connection_id, connection) in &story.connections {
                if connection.id != *connection_id {
                    errors.push(format!(
                        "story {story_id} connection {connection_id} has mismatched id {}",
                        connection.id
                    ));
                }
                if !story.nodes.contains_key(&connection.from_node_id) {
                    errors.push(format!(
                        "story {story_id} connection {connection_id} has missing source node {}",
                        connection.from_node_id
                    ));
                }
                if !story.nodes.contains_key(&connection.to_node_id) {
                    errors.push(format!(
                        "story {story_id} connection {connection_id} has missing target node {}",
                        connection.to_node_id
                    ));
                }
                if !["next", "choice", "condition"].contains(&connection.connection_kind.as_str()) {
                    errors.push(format!(
                        "story {story_id} connection {connection_id} has unknown kind {}",
                        connection.connection_kind
                    ));
                }
                let slot = (
                    connection.from_node_id.clone(),
                    connection.output_id.clone(),
                    connection.to_node_id.clone(),
                );
                if !connection_slots.insert(slot) {
                    errors.push(format!(
                        "story {story_id} has duplicate connection slot in {connection_id}"
                    ));
                }
                incoming.insert(connection.to_node_id.clone());
            }
            for node_id in story.nodes.keys() {
                if node_id != &story.entry_node_id && !incoming.contains(node_id) {
                    errors.push(format!("story {story_id} node {node_id} is unreachable"));
                }
            }
        }

        for (dialogue_id, dialogue) in &self.dialogues {
            if let Some(story_id) = &dialogue.story_id {
                if !self.stories.contains_key(story_id) {
                    errors.push(format!(
                        "dialogue {dialogue_id} references missing story {story_id}"
                    ));
                }
            }
            let mut line_ids = BTreeSet::new();
            for line in &dialogue.lines {
                if line.id.is_empty() || !line_ids.insert(line.id.clone()) {
                    errors.push(format!(
                        "dialogue {dialogue_id} has duplicate or empty line id"
                    ));
                }
                if line.kind == "text" {
                    if let Some(key) = &line.key {
                        if !source_keys.contains(key) {
                            errors.push(format!(
                                "dialogue {dialogue_id} references missing text key {key}"
                            ));
                        }
                    } else {
                        errors.push(format!(
                            "dialogue {dialogue_id} has a text line without a key"
                        ));
                    }
                } else if line.kind == "command" {
                    errors.push(format!(
                        "dialogue {dialogue_id} contains legacy command line {}; use a command node",
                        line.id
                    ));
                }
                if let Some(action) = &line.on_enter {
                    validate_action(
                        &mut errors,
                        &format!("dialogue {dialogue_id} line {}", line.id),
                        action,
                        self,
                    );
                }
                if let Some(action) = &line.on_exit {
                    validate_action(
                        &mut errors,
                        &format!("dialogue {dialogue_id} line {}", line.id),
                        action,
                        self,
                    );
                }
            }
        }

        for (quest_id, quest) in &self.quests {
            if let Some(story_id) = &quest.story_id {
                if !self.stories.contains_key(story_id) {
                    errors.push(format!(
                        "quest {quest_id} references missing story {story_id}"
                    ));
                }
            }
            if quest.name_key.is_empty() || !source_keys.contains(&quest.name_key) {
                errors.push(format!(
                    "quest {quest_id} has missing name key {}",
                    quest.name_key
                ));
            }
            for requirement in &quest.requirements {
                if !["ITEM_COLLECT", "TALK_WITH_NPC", "REACH_AREA", "CUSTOM"]
                    .contains(&requirement.requirement_type.as_str())
                {
                    errors.push(format!(
                        "quest {quest_id} has unknown requirement {}",
                        requirement.requirement_type
                    ));
                }
                if requirement.requirement_type == "CUSTOM" {
                    validate_callback(
                        &mut errors,
                        &format!("quest {quest_id} requirement"),
                        requirement.callback_id.as_deref(),
                        &self.callbacks,
                    );
                }
                if let Some(item_id) = &requirement.item_id {
                    if !self.items.contains_key(item_id) {
                        errors.push(format!(
                            "quest {quest_id} references missing item {item_id}"
                        ));
                    }
                }
                if let Some(entity_id) = &requirement.entity_id {
                    if !self.entities.contains_key(entity_id) {
                        errors.push(format!(
                            "quest {quest_id} references missing entity {entity_id}"
                        ));
                    }
                }
            }
            for action in quest
                .start_actions
                .iter()
                .chain(quest.complete_actions.iter())
                .chain(quest.fail_actions.iter())
            {
                validate_action(&mut errors, &format!("quest {quest_id}"), action, self);
            }
        }
        for (entity_id, entity) in &self.entities {
            for dialogue_id in &entity.dialogue_ids {
                if !self.dialogues.contains_key(dialogue_id) {
                    errors.push(format!(
                        "entity {entity_id} references missing dialogue {dialogue_id}"
                    ));
                }
            }
            for condition in &entity.interaction_conditions {
                validate_condition(
                    &mut errors,
                    &format!("entity {entity_id}"),
                    condition,
                    &self.callbacks,
                );
            }
            for action in &entity.interaction_actions {
                validate_action(&mut errors, &format!("entity {entity_id}"), action, self);
            }
        }
        errors
    }

    fn normalize_legacy_line_ids(&mut self) {
        for (dialogue_id, dialogue) in &mut self.dialogues {
            for (index, line) in dialogue.lines.iter_mut().enumerate() {
                if line.id.is_empty() {
                    line.id = line
                        .key
                        .clone()
                        .unwrap_or_else(|| format!("{dialogue_id}.line_{index}"));
                }
            }
        }
        for (quest_id, quest) in &mut self.quests {
            if quest.name_key.is_empty() {
                quest.name_key = quest.description_keys.first().cloned().unwrap_or_default();
            }
            if quest.requirements.is_empty() {
                quest.requirements = legacy_requirements(quest_id);
            }
        }
    }

    fn normalize_legacy_connections(&mut self) {
        for story in self.stories.values_mut() {
            if !story.connections.is_empty() {
                continue;
            }
            let mut ordinal = 0usize;
            let nodes = story.nodes.clone();
            for (from_node_id, node) in nodes {
                for to_node_id in node.next {
                    let id = format!("edge.{from_node_id}.default.{ordinal}");
                    ordinal += 1;
                    story.connections.insert(
                        id.clone(),
                        ConnectionDefinition {
                            id,
                            from_node_id: from_node_id.clone(),
                            to_node_id,
                            output_id: "default".into(),
                            connection_kind: "next".into(),
                            label_key: None,
                            condition: None,
                        },
                    );
                }
                for choice in node.choices {
                    let id = format!("edge.{from_node_id}.choice.{}.{}", choice.id, ordinal);
                    ordinal += 1;
                    story.connections.insert(
                        id.clone(),
                        ConnectionDefinition {
                            id,
                            from_node_id: from_node_id.clone(),
                            to_node_id: choice.next,
                            output_id: choice.id,
                            connection_kind: "choice".into(),
                            label_key: (!choice.label_key.is_empty()).then_some(choice.label_key),
                            condition: choice.condition,
                        },
                    );
                }
            }
        }
    }

    fn normalize_legacy_payloads(&mut self) -> Result<(), String> {
        let mut legacy_commands = BTreeMap::new();
        for (dialogue_id, dialogue) in &self.dialogues {
            for line in &dialogue.lines {
                if line.kind == "command" {
                    if let Some(command) = &line.command {
                        legacy_commands
                            .insert((dialogue_id.clone(), line.id.clone()), command.clone());
                    }
                }
            }
        }
        let first_text_lines: BTreeMap<String, String> = self
            .dialogues
            .iter()
            .filter_map(|(dialogue_id, dialogue)| {
                dialogue
                    .lines
                    .iter()
                    .find(|line| line.kind == "text")
                    .map(|line| (dialogue_id.clone(), line.id.clone()))
            })
            .collect();
        let mut converted_commands = BTreeSet::new();
        for story in self.stories.values_mut() {
            for node in story.nodes.values_mut() {
                if let Some(dialogue_id) = &node.dialogue_id {
                    if let Some(line_id) = &node.dialogue_line_id {
                        if let Some(command) =
                            legacy_commands.get(&(dialogue_id.clone(), line_id.clone()))
                        {
                            converted_commands.insert((dialogue_id.clone(), line_id.clone()));
                            let (node_type, payload) =
                                legacy_command_payload(command, &self.items)?;
                            node.node_type = node_type;
                            node.action = match &payload {
                                NodePayload::Action(action) => Some(action.clone()),
                                _ => None,
                            };
                            node.payload = Some(payload);
                            node.dialogue_id = None;
                            node.dialogue_line_id = None;
                            continue;
                        }
                    }
                }
                if node.payload.is_some() {
                    continue;
                }
                node.payload =
                    Some(match node.node_type.as_str() {
                        "dialogue" => NodePayload::Dialogue {
                            dialogue_id: node.dialogue_id.clone().ok_or_else(|| {
                                "dialogue node is missing dialogue_id".to_string()
                            })?,
                            line_id: node
                                .dialogue_line_id
                                .clone()
                                .or_else(|| {
                                    node.dialogue_id
                                        .as_ref()
                                        .and_then(|id| first_text_lines.get(id).cloned())
                                })
                                .ok_or_else(|| {
                                    "dialogue node is missing dialogue_line_id".to_string()
                                })?,
                        },
                        "show_item" => {
                            let item_id = node
                                .action
                                .as_ref()
                                .and_then(|action| match action {
                                    ActionDefinition::GiveItem { item_id, .. }
                                    | ActionDefinition::RemoveItem { item_id, .. } => {
                                        Some(item_id.clone())
                                    }
                                    _ => None,
                                })
                                .ok_or_else(|| "show_item node is missing item_id".to_string())?;
                            NodePayload::ShowItem { item_id }
                        }
                        "show_icon" => {
                            let icon_id = node
                                .action
                                .as_ref()
                                .and_then(|action| match action {
                                    ActionDefinition::LegacyShowIcon { icon_id } => {
                                        Some(icon_id.clone())
                                    }
                                    _ => None,
                                })
                                .ok_or_else(|| "show_icon node is missing icon_id".to_string())?;
                            NodePayload::ShowIcon { icon_id }
                        }
                        "action" => {
                            NodePayload::Action(node.action.clone().ok_or_else(|| {
                                "action node is missing action payload".to_string()
                            })?)
                        }
                        "condition" => NodePayload::Condition(node.condition.clone().unwrap_or(
                            ConditionDefinition {
                                condition_type: "always".into(),
                                params: BTreeMap::new(),
                                callback_id: None,
                            },
                        )),
                        "choice" => NodePayload::Choice,
                        "quest_start" => NodePayload::QuestStart {
                            quest_id: node.quest_id.clone().ok_or_else(|| {
                                "quest_start node is missing quest_id".to_string()
                            })?,
                        },
                        "quest_update" => NodePayload::QuestUpdate {
                            quest_id: node.quest_id.clone().ok_or_else(|| {
                                "quest_update node is missing quest_id".to_string()
                            })?,
                        },
                        "scene_transition" => NodePayload::SceneTransition {
                            scene_id: node.scene_id.clone().ok_or_else(|| {
                                "scene_transition node is missing scene_id".to_string()
                            })?,
                        },
                        "event" => NodePayload::Event {
                            event_id: node
                                .event_id
                                .clone()
                                .ok_or_else(|| "event node is missing event_id".to_string())?,
                        },
                        "end" => NodePayload::End,
                        other => return Err(format!("unknown node type {other}")),
                    });
            }
        }
        for (dialogue_id, dialogue) in self.dialogues.iter_mut() {
            dialogue.lines.retain(|line| {
                line.kind != "command"
                    || !converted_commands.contains(&(dialogue_id.clone(), line.id.clone()))
            });
        }
        Ok(())
    }
}

fn legacy_command_payload(
    command: &str,
    items: &BTreeMap<String, ItemDefinition>,
) -> Result<(String, NodePayload), String> {
    let body = command.trim().trim_start_matches('[').trim_end_matches(']');
    if let Some(item_id) = body.strip_prefix("SHOW:") {
        if items.contains_key(item_id) {
            return Ok((
                "show_item".into(),
                NodePayload::ShowItem {
                    item_id: item_id.into(),
                },
            ));
        }
        return Ok((
            "show_icon".into(),
            NodePayload::ShowIcon {
                icon_id: item_id.into(),
            },
        ));
    }
    match body {
        "START_CAMERA_SHAKE" => Ok((
            "action".into(),
            NodePayload::Action(ActionDefinition::CameraShakeStart),
        )),
        "STOP_CAMERA_SHAKE" => Ok((
            "action".into(),
            NodePayload::Action(ActionDefinition::CameraShakeStop),
        )),
        unknown => Err(format!(
            "cannot migrate unknown dialogue command [{unknown}]; register it as a typed node first"
        )),
    }
}

fn project_connections_to_legacy_fields(story: &mut StoryDefinition) {
    for node in story.nodes.values_mut() {
        node.next.clear();
        node.choices.clear();
    }
    for connection in story.connections.values() {
        let Some(node) = story.nodes.get_mut(&connection.from_node_id) else {
            continue;
        };
        if connection.connection_kind == "choice" {
            node.choices.push(ChoiceDefinition {
                id: connection.output_id.clone(),
                label_key: connection.label_key.clone().unwrap_or_default(),
                next: connection.to_node_id.clone(),
                condition: connection.condition.clone(),
            });
        } else {
            node.next.push(connection.to_node_id.clone());
        }
    }
}

fn canonicalize_story_nodes(story: &mut StoryDefinition) {
    for node in story.nodes.values_mut() {
        let Some(payload) = node.payload.clone() else {
            continue;
        };
        match payload {
            NodePayload::Dialogue {
                dialogue_id,
                line_id,
            } => {
                node.node_type = "dialogue".into();
                node.dialogue_id = Some(dialogue_id);
                node.dialogue_line_id = Some(line_id);
                node.action = None;
                node.condition = None;
            }
            NodePayload::ShowItem { .. } | NodePayload::ShowIcon { .. } => {
                node.dialogue_id = None;
                node.dialogue_line_id = None;
                node.action = None;
                node.condition = None;
            }
            NodePayload::Action(action) => {
                node.node_type = "action".into();
                node.action = Some(action);
                node.dialogue_id = None;
                node.dialogue_line_id = None;
                node.condition = None;
            }
            NodePayload::Condition(condition) => {
                node.node_type = "condition".into();
                node.condition = Some(condition);
                node.action = None;
            }
            NodePayload::Choice => {
                node.node_type = "choice".into();
                node.action = None;
                node.condition = None;
            }
            NodePayload::QuestStart { quest_id } => {
                node.node_type = "quest_start".into();
                node.quest_id = Some(quest_id);
                node.action = None;
            }
            NodePayload::QuestUpdate { quest_id } => {
                node.node_type = "quest_update".into();
                node.quest_id = Some(quest_id);
                node.action = None;
            }
            NodePayload::SceneTransition { scene_id } => {
                node.node_type = "scene_transition".into();
                node.scene_id = Some(scene_id);
                node.action = None;
            }
            NodePayload::Event { event_id } => {
                node.node_type = "event".into();
                node.event_id = Some(event_id);
                node.action = None;
            }
            NodePayload::End => {
                node.node_type = "end".into();
                node.action = None;
                node.condition = None;
            }
        }
    }
}

/// Computes a deterministic top-down layout without mutating the catalog.
/// Reachable nodes are placed by graph depth; nodes that are not reachable
/// from the entry node are placed in a separate diagnostics area below it.
pub fn arrange_story_top_down(
    story: &StoryDefinition,
) -> (BTreeMap<String, StoryLayoutPosition>, StoryLayoutIssues) {
    let mut positions = BTreeMap::new();
    let mut levels: BTreeMap<usize, BTreeSet<String>> = BTreeMap::new();
    let mut depth_by_node = BTreeMap::new();
    let mut frontier = vec![story.entry_node_id.clone()];
    depth_by_node.insert(story.entry_node_id.clone(), 0usize);

    while let Some(node_id) = frontier.pop() {
        let depth = depth_by_node[&node_id];
        levels.entry(depth).or_default().insert(node_id.clone());
        let Some(_) = story.nodes.get(&node_id) else {
            continue;
        };
        let mut children = outgoing_node_ids(story, &node_id);
        children.sort();
        children.dedup();
        for child in children.into_iter().rev() {
            if !depth_by_node.contains_key(&child) && story.nodes.contains_key(&child) {
                depth_by_node.insert(child.clone(), depth + 1);
                frontier.push(child);
            }
        }
    }

    let mut max_level = 0usize;
    for (level, nodes) in &levels {
        max_level = max_level.max(*level);
        let total_width = nodes.len() as f32 * NODE_WIDTH
            + nodes.len().saturating_sub(1) as f32 * LAYOUT_HORIZONTAL_GAP;
        let start_x = (720.0 - total_width / 2.0).max(48.0);
        for (index, node_id) in nodes.iter().enumerate() {
            positions.insert(
                node_id.clone(),
                StoryLayoutPosition {
                    x: start_x + index as f32 * (NODE_WIDTH + LAYOUT_HORIZONTAL_GAP),
                    y: 48.0 + *level as f32 * (NODE_HEIGHT + LAYOUT_VERTICAL_GAP),
                },
            );
        }
    }

    let orphaned_nodes: Vec<String> = story
        .nodes
        .keys()
        .filter(|node_id| !depth_by_node.contains_key(*node_id))
        .cloned()
        .collect();
    let orphan_y = 48.0 + (max_level + 1) as f32 * (NODE_HEIGHT + LAYOUT_VERTICAL_GAP);
    for (index, node_id) in orphaned_nodes.iter().enumerate() {
        positions.insert(
            node_id.clone(),
            StoryLayoutPosition {
                x: 48.0 + index as f32 * (NODE_WIDTH + LAYOUT_HORIZONTAL_GAP),
                y: orphan_y,
            },
        );
    }

    let mut cycle_edges = Vec::new();
    for (node_id, _) in &story.nodes {
        let Some(source_depth) = depth_by_node.get(node_id) else {
            continue;
        };
        let children = outgoing_node_ids(story, node_id);
        for child in children {
            if depth_by_node
                .get(&child)
                .is_some_and(|target_depth| target_depth <= source_depth)
            {
                cycle_edges.push((node_id.clone(), child));
            }
        }
    }
    cycle_edges.sort();
    cycle_edges.dedup();

    (
        positions,
        StoryLayoutIssues {
            orphaned_nodes,
            cycle_edges,
        },
    )
}

pub fn outgoing_node_ids(story: &StoryDefinition, node_id: &str) -> Vec<String> {
    if !story.connections.is_empty() {
        return story
            .connections
            .values()
            .filter(|connection| connection.from_node_id == node_id)
            .map(|connection| connection.to_node_id.clone())
            .collect();
    }
    story
        .nodes
        .get(node_id)
        .map(|node| {
            node.next
                .iter()
                .cloned()
                .chain(node.choices.iter().map(|choice| choice.next.clone()))
                .collect()
        })
        .unwrap_or_default()
}

pub fn node_type_color(node_type: &str) -> [u8; 3] {
    match node_type {
        "dialogue" => [52, 129, 203],
        "show_item" => [123, 78, 170],
        "show_icon" => [154, 74, 145],
        "choice" => [135, 88, 194],
        "condition" => [196, 158, 46],
        "action" => [208, 116, 45],
        "quest_start" | "quest_update" => [52, 156, 93],
        "scene_transition" => [38, 165, 166],
        "event" => [187, 67, 150],
        "end" => [99, 111, 125],
        _ => [81, 96, 113],
    }
}

fn legacy_requirements(quest_id: &str) -> Vec<RequirementDefinition> {
    let item = |item_id: &str| RequirementDefinition {
        requirement_type: "ITEM_COLLECT".into(),
        item_id: Some(item_id.into()),
        entity_id: None,
        amount: Some(1),
        area_id: None,
        callback_id: None,
    };
    let callback = |callback_id: &str| RequirementDefinition {
        requirement_type: "CUSTOM".into(),
        item_id: None,
        entity_id: None,
        amount: None,
        area_id: None,
        callback_id: Some(callback_id.into()),
    };
    match quest_id {
        "craft_bucket" => vec![item("BUCKET")],
        "craft_hammer" => vec![item("HAMMER")],
        "npc_hammer" => vec![item("BLACKSMITH_HAMMER")],
        "oldman_shrine" => vec![item("SHRINE_RUNE")],
        "repair_entity" => vec![callback("legacy.repair_entity")],
        "repair_house" => vec![callback("legacy.repair_house")],
        "farming" => vec![callback("story.farming.quest_plots")],
        "shovel_pots" => vec![callback("story.farming.mark_nearest_plot")],
        "save_blacksmith" => vec![callback("story.blacksmith.rescue")],
        _ => Vec::new(),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StoryDefinition {
    #[serde(default)]
    pub name_key: String,
    #[serde(default)]
    pub description_key: String,
    #[serde(default)]
    pub owner_id: String,
    #[serde(default)]
    pub entry_node_id: String,
    #[serde(default)]
    pub chapters: Vec<ChapterDefinition>,
    #[serde(default)]
    pub nodes: BTreeMap<String, StoryNode>,
    #[serde(default)]
    pub connections: BTreeMap<String, ConnectionDefinition>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChapterDefinition {
    pub id: String,
    #[serde(default)]
    pub name_key: String,
    #[serde(default)]
    pub node_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StoryNode {
    pub node_type: String,
    #[serde(default)]
    pub name_key: String,
    /// Typed content for the node. The legacy fields below remain readable
    /// while catalogs migrate from schema v2, but new authoring operations
    /// must populate this payload.
    #[serde(default)]
    pub payload: Option<NodePayload>,
    #[serde(default)]
    pub chapter_id: Option<String>,
    #[serde(default)]
    pub dialogue_id: Option<String>,
    /// Stable dialogue-line identity for sequence-oriented story scenes.
    /// It is optional so legacy nodes can continue to represent a whole dialogue.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dialogue_line_id: Option<String>,
    #[serde(default)]
    pub quest_id: Option<String>,
    #[serde(default)]
    pub scene_id: Option<String>,
    #[serde(default)]
    pub event_id: Option<String>,
    #[serde(default)]
    pub action: Option<ActionDefinition>,
    #[serde(default)]
    pub condition: Option<ConditionDefinition>,
    #[serde(default)]
    pub choices: Vec<ChoiceDefinition>,
    #[serde(default)]
    pub next: Vec<String>,
    #[serde(default = "default_node_position")]
    pub position: [f32; 2],
}

fn default_node_position() -> [f32; 2] {
    [80.0, 80.0]
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChoiceDefinition {
    pub id: String,
    #[serde(default)]
    pub label_key: String,
    pub next: String,
    #[serde(default)]
    pub condition: Option<ConditionDefinition>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DialogueDefinition {
    #[serde(default)]
    pub name_key: String,
    #[serde(default)]
    pub story_id: Option<String>,
    #[serde(default)]
    pub chapter_id: Option<String>,
    #[serde(default)]
    pub owner_id: Option<String>,
    #[serde(default)]
    pub lines: Vec<DialogueLineDefinition>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub on_enter: Vec<ActionDefinition>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub on_exit: Vec<ActionDefinition>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DialogueLineDefinition {
    #[serde(default)]
    pub id: String,
    pub kind: String,
    #[serde(default)]
    pub key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(default)]
    pub speaker: Option<String>,
    #[serde(default)]
    pub variables: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_enter: Option<ActionDefinition>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_exit: Option<ActionDefinition>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", content = "data")]
#[serde(rename_all = "snake_case")]
pub enum NodePayload {
    Dialogue {
        dialogue_id: String,
        line_id: String,
    },
    ShowItem {
        item_id: String,
    },
    ShowIcon {
        icon_id: String,
    },
    Choice,
    Condition(ConditionDefinition),
    Action(ActionDefinition),
    QuestStart {
        quest_id: String,
    },
    QuestUpdate {
        quest_id: String,
    },
    SceneTransition {
        scene_id: String,
    },
    Event {
        event_id: String,
    },
    End,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuestDefinition {
    #[serde(default)]
    pub name_key: String,
    #[serde(default)]
    pub description_keys: Vec<String>,
    #[serde(default)]
    pub story_id: Option<String>,
    #[serde(default)]
    pub chapter_id: Option<String>,
    #[serde(default)]
    pub owner_id: Option<String>,
    #[serde(default)]
    pub stages: Vec<QuestStageDefinition>,
    #[serde(default)]
    pub requirements: Vec<RequirementDefinition>,
    #[serde(default)]
    pub start_actions: Vec<ActionDefinition>,
    #[serde(default)]
    pub complete_actions: Vec<ActionDefinition>,
    #[serde(default)]
    pub fail_actions: Vec<ActionDefinition>,
    #[serde(default)]
    pub initial_status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuestStageDefinition {
    pub id: String,
    #[serde(default)]
    pub name_key: String,
    #[serde(default)]
    pub description_key: String,
    #[serde(default)]
    pub requirements: Vec<RequirementDefinition>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequirementDefinition {
    pub requirement_type: String,
    #[serde(default)]
    pub item_id: Option<String>,
    #[serde(default)]
    pub entity_id: Option<String>,
    #[serde(default)]
    pub amount: Option<i32>,
    #[serde(default)]
    pub area_id: Option<String>,
    #[serde(default)]
    pub callback_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemDefinition {
    #[serde(default)]
    pub name_key: String,
    #[serde(default)]
    pub description_key: String,
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub sprite_ref: String,
    #[serde(default = "default_true")]
    pub stackable: bool,
    #[serde(default)]
    pub max_stack: Option<i32>,
    #[serde(default)]
    pub effects: Vec<ActionDefinition>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub quest_item: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityDefinition {
    #[serde(default)]
    pub name_key: String,
    #[serde(default)]
    pub entity_type: String,
    #[serde(default)]
    pub sprite_ref: String,
    #[serde(default)]
    pub owner_id: String,
    #[serde(default)]
    pub dialogue_ids: Vec<String>,
    #[serde(default)]
    pub interaction_conditions: Vec<ConditionDefinition>,
    #[serde(default)]
    pub interaction_actions: Vec<ActionDefinition>,
    #[serde(default)]
    pub states: Vec<String>,
    #[serde(default)]
    pub initial_state: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "type", content = "data")]
#[serde(rename_all = "snake_case")]
pub enum ActionDefinition {
    StartDialogue {
        dialogue_id: String,
    },
    StartQuest {
        quest_id: String,
    },
    CompleteQuest {
        quest_id: String,
    },
    GiveItem {
        item_id: String,
        amount: u32,
    },
    RemoveItem {
        item_id: String,
        amount: u32,
    },
    SetFlag {
        flag: String,
    },
    ClearFlag {
        flag: String,
    },
    TriggerEvent {
        event_id: String,
    },
    ChangeScene {
        scene_id: String,
    },
    SpawnEntity {
        entity_id: String,
    },
    DespawnEntity {
        entity_id: String,
    },
    SetEntityState {
        entity_id: String,
        state: String,
    },
    CameraShakeStart,
    CameraShakeStop,
    EnableGameplayButton {
        button_id: String,
    },
    Callback {
        callback_id: String,
        #[serde(default)]
        params: BTreeMap<String, Value>,
    },
    LegacyShowItem {
        item_id: String,
    },
    LegacyShowIcon {
        icon_id: String,
    },
}

impl<'de> Deserialize<'de> for ActionDefinition {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;
        parse_action_value(&value).map_err(serde::de::Error::custom)
    }
}

impl ActionDefinition {
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::StartDialogue { .. } => "start_dialogue",
            Self::StartQuest { .. } => "start_quest",
            Self::CompleteQuest { .. } => "complete_quest",
            Self::GiveItem { .. } => "give_item",
            Self::RemoveItem { .. } => "remove_item",
            Self::SetFlag { .. } => "set_flag",
            Self::ClearFlag { .. } => "clear_flag",
            Self::TriggerEvent { .. } => "trigger_event",
            Self::ChangeScene { .. } => "change_scene",
            Self::SpawnEntity { .. } => "spawn_entity",
            Self::DespawnEntity { .. } => "despawn_entity",
            Self::SetEntityState { .. } => "set_entity_state",
            Self::CameraShakeStart => "camera_shake_start",
            Self::CameraShakeStop => "camera_shake_stop",
            Self::EnableGameplayButton { .. } => "enable_gameplay_button",
            Self::Callback { .. } => "callback",
            Self::LegacyShowItem { .. } => "show_item",
            Self::LegacyShowIcon { .. } => "show_icon",
        }
    }
}

pub fn action_from_type(action_type: &str) -> Option<ActionDefinition> {
    Some(match action_type {
        "start_dialogue" => ActionDefinition::StartDialogue {
            dialogue_id: String::new(),
        },
        "start_quest" => ActionDefinition::StartQuest {
            quest_id: String::new(),
        },
        "complete_quest" => ActionDefinition::CompleteQuest {
            quest_id: String::new(),
        },
        "give_item" => ActionDefinition::GiveItem {
            item_id: String::new(),
            amount: 1,
        },
        "remove_item" => ActionDefinition::RemoveItem {
            item_id: String::new(),
            amount: 1,
        },
        "set_flag" => ActionDefinition::SetFlag {
            flag: String::new(),
        },
        "clear_flag" => ActionDefinition::ClearFlag {
            flag: String::new(),
        },
        "trigger_event" => ActionDefinition::TriggerEvent {
            event_id: String::new(),
        },
        "change_scene" => ActionDefinition::ChangeScene {
            scene_id: String::new(),
        },
        "spawn_entity" => ActionDefinition::SpawnEntity {
            entity_id: String::new(),
        },
        "despawn_entity" => ActionDefinition::DespawnEntity {
            entity_id: String::new(),
        },
        "set_entity_state" => ActionDefinition::SetEntityState {
            entity_id: String::new(),
            state: String::new(),
        },
        "camera_shake_start" | "camera_shake" => ActionDefinition::CameraShakeStart,
        "camera_shake_stop" => ActionDefinition::CameraShakeStop,
        "enable_gameplay_button" => ActionDefinition::EnableGameplayButton {
            button_id: String::new(),
        },
        "callback" => ActionDefinition::Callback {
            callback_id: String::new(),
            params: BTreeMap::new(),
        },
        _ => return None,
    })
}

fn parse_action_value(value: &Value) -> Result<ActionDefinition, String> {
    let action_type = value
        .get("type")
        .and_then(Value::as_str)
        .or_else(|| value.get("action_type").and_then(Value::as_str))
        .ok_or_else(|| "action is missing type".to_string())?;
    let data = value.get("data").unwrap_or(value);
    let params = value.get("params").unwrap_or(data);
    let string = |key: &str| {
        data.get(key)
            .or_else(|| params.get(key))
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| format!("{action_type} is missing {key}"))
    };
    let amount = || {
        data.get("amount")
            .or_else(|| params.get("amount"))
            .and_then(Value::as_u64)
            .map(|value| value as u32)
            .unwrap_or(1)
    };
    match action_type {
        "start_dialogue" => Ok(ActionDefinition::StartDialogue {
            dialogue_id: string("dialogue_id")?,
        }),
        "start_quest" => Ok(ActionDefinition::StartQuest {
            quest_id: string("quest_id")?,
        }),
        "complete_quest" => Ok(ActionDefinition::CompleteQuest {
            quest_id: string("quest_id")?,
        }),
        "give_item" => Ok(ActionDefinition::GiveItem {
            item_id: string("item_id")?,
            amount: amount(),
        }),
        "remove_item" => Ok(ActionDefinition::RemoveItem {
            item_id: string("item_id")?,
            amount: amount(),
        }),
        "set_flag" => Ok(ActionDefinition::SetFlag {
            flag: string("flag")?,
        }),
        "clear_flag" => Ok(ActionDefinition::ClearFlag {
            flag: string("flag")?,
        }),
        "trigger_event" => Ok(ActionDefinition::TriggerEvent {
            event_id: string("event_id")?,
        }),
        "change_scene" => Ok(ActionDefinition::ChangeScene {
            scene_id: string("scene_id")?,
        }),
        "spawn_entity" => Ok(ActionDefinition::SpawnEntity {
            entity_id: string("entity_id")?,
        }),
        "despawn_entity" => Ok(ActionDefinition::DespawnEntity {
            entity_id: string("entity_id")?,
        }),
        "set_entity_state" => Ok(ActionDefinition::SetEntityState {
            entity_id: string("entity_id")?,
            state: string("state")?,
        }),
        "camera_shake_start" => Ok(ActionDefinition::CameraShakeStart),
        "camera_shake_stop" => Ok(ActionDefinition::CameraShakeStop),
        "camera_shake" => match string("phase")?.as_str() {
            "start" => Ok(ActionDefinition::CameraShakeStart),
            "stop" => Ok(ActionDefinition::CameraShakeStop),
            phase => Err(format!("camera_shake has invalid phase {phase}")),
        },
        "enable_gameplay_button" => Ok(ActionDefinition::EnableGameplayButton {
            button_id: string("button_id")?,
        }),
        "callback" => Ok(ActionDefinition::Callback {
            callback_id: value
                .get("callback_id")
                .or_else(|| data.get("callback_id"))
                .and_then(Value::as_str)
                .ok_or_else(|| "callback is missing callback_id".to_string())?
                .into(),
            params: data
                .get("params")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .collect(),
        }),
        "show_item" => Ok(ActionDefinition::LegacyShowItem {
            item_id: string("item_id")?,
        }),
        "show_icon" => Ok(ActionDefinition::LegacyShowIcon {
            icon_id: string("icon_id")?,
        }),
        other => Err(format!("unknown action {other}")),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConditionDefinition {
    pub condition_type: String,
    #[serde(default)]
    pub params: BTreeMap<String, Value>,
    #[serde(default)]
    pub callback_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallbackDefinition {
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub registered_in: String,
}

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

fn validate_node_payload(
    errors: &mut Vec<String>,
    context: &str,
    node_type: &str,
    payload: &NodePayload,
    catalog: &ContentCatalog,
) {
    let mismatch = |errors: &mut Vec<String>| {
        errors.push(format!(
            "{context} has payload incompatible with node type {node_type}"
        ));
    };
    match (node_type, payload) {
        (
            "dialogue",
            NodePayload::Dialogue {
                dialogue_id,
                line_id,
            },
        ) => {
            let Some(dialogue) = catalog.dialogues.get(dialogue_id) else {
                errors.push(format!(
                    "{context} references missing dialogue {dialogue_id}"
                ));
                return;
            };
            if !dialogue
                .lines
                .iter()
                .any(|line| line.id == *line_id && line.kind == "text")
            {
                errors.push(format!(
                    "{context} references a missing or non-text dialogue line {line_id}"
                ));
            }
        }
        ("show_item", NodePayload::ShowItem { item_id }) => {
            if !catalog.items.contains_key(item_id) {
                errors.push(format!("{context} references missing item {item_id}"));
            }
        }
        ("show_icon", NodePayload::ShowIcon { .. }) => {}
        ("choice", NodePayload::Choice)
        | ("condition", NodePayload::Condition(_))
        | ("action", NodePayload::Action(_))
        | ("quest_start", NodePayload::QuestStart { .. })
        | ("quest_update", NodePayload::QuestUpdate { .. })
        | ("scene_transition", NodePayload::SceneTransition { .. })
        | ("event", NodePayload::Event { .. })
        | ("end", NodePayload::End) => {}
        _ => mismatch(errors),
    }
    if let NodePayload::Action(action) = payload {
        validate_action(errors, context, action, catalog);
    }
}

fn validate_action(
    errors: &mut Vec<String>,
    context: &str,
    action: &ActionDefinition,
    catalog: &ContentCatalog,
) {
    let missing_item = |item_id: &str, errors: &mut Vec<String>| {
        if !catalog.items.contains_key(item_id) {
            errors.push(format!("{context} references missing item {item_id}"));
        }
    };
    let missing_entity = |entity_id: &str, errors: &mut Vec<String>| {
        if !catalog.entities.contains_key(entity_id) {
            errors.push(format!("{context} references missing entity {entity_id}"));
        }
    };
    let missing_quest = |quest_id: &str, errors: &mut Vec<String>| {
        if !catalog.quests.contains_key(quest_id) {
            errors.push(format!("{context} references missing quest {quest_id}"));
        }
    };
    match action {
        ActionDefinition::StartDialogue { dialogue_id } => {
            if !catalog.dialogues.contains_key(dialogue_id) {
                errors.push(format!(
                    "{context} references missing dialogue {dialogue_id}"
                ));
            }
        }
        ActionDefinition::StartQuest { quest_id }
        | ActionDefinition::CompleteQuest { quest_id } => missing_quest(quest_id, errors),
        ActionDefinition::GiveItem { item_id, .. }
        | ActionDefinition::RemoveItem { item_id, .. } => missing_item(item_id, errors),
        ActionDefinition::SpawnEntity { entity_id }
        | ActionDefinition::DespawnEntity { entity_id }
        | ActionDefinition::SetEntityState { entity_id, .. } => missing_entity(entity_id, errors),
        ActionDefinition::Callback { callback_id, .. } => {
            validate_callback(errors, context, Some(callback_id), &catalog.callbacks)
        }
        ActionDefinition::SetFlag { .. }
        | ActionDefinition::ClearFlag { .. }
        | ActionDefinition::TriggerEvent { .. }
        | ActionDefinition::ChangeScene { .. }
        | ActionDefinition::CameraShakeStart
        | ActionDefinition::CameraShakeStop
        | ActionDefinition::EnableGameplayButton { .. } => {}
        ActionDefinition::LegacyShowItem { item_id } => {
            errors.push(format!(
                "{context} uses legacy show_item action; use a show_item node"
            ));
            missing_item(item_id, errors);
        }
        ActionDefinition::LegacyShowIcon { .. } => {
            errors.push(format!(
                "{context} uses legacy show_icon action; use a show_icon node"
            ));
        }
    }
}

fn validate_condition(
    errors: &mut Vec<String>,
    context: &str,
    condition: &ConditionDefinition,
    callbacks: &BTreeMap<String, CallbackDefinition>,
) {
    if !CONDITION_TYPES.contains(&condition.condition_type.as_str()) {
        errors.push(format!(
            "{context} has unknown condition {}",
            condition.condition_type
        ));
    }
    if condition.condition_type == "callback" {
        validate_callback(errors, context, condition.callback_id.as_deref(), callbacks);
    }
}

fn validate_callback(
    errors: &mut Vec<String>,
    context: &str,
    callback_id: Option<&str>,
    callbacks: &BTreeMap<String, CallbackDefinition>,
) {
    match callback_id {
        Some(callback_id) if callbacks.contains_key(callback_id) => {}
        Some(callback_id) => errors.push(format!(
            "{context} references missing callback {callback_id}"
        )),
        None => errors.push(format!("{context} requires a callback_id")),
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
    fs::write(&path, format!("{text}\n")).map_err(|error| format!("{}: {error}", path.display()))
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
        super::project_connections_to_legacy_fields(&mut projected);
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
