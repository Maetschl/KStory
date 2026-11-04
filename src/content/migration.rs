use super::model::legacy_requirements;
use super::*;
use std::collections::{BTreeMap, BTreeSet};

impl ContentCatalog {
    pub(super) fn normalize_legacy_line_ids(&mut self) {
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

    pub(super) fn normalize_legacy_connections(&mut self) {
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

    pub(super) fn normalize_legacy_payloads(&mut self) -> Result<(), String> {
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

pub(super) fn project_connections_to_legacy_fields(story: &mut StoryDefinition) {
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

pub(super) fn canonicalize_story_nodes(story: &mut StoryDefinition) {
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
