use super::*;
use std::collections::{BTreeMap, BTreeSet};

impl ContentCatalog {
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
                    if !story.nodes.contains_key(next) {
                        errors.push(format!(
                            "story {story_id} node {node_id} points to missing node {next}"
                        ));
                    }
                }
                for choice in &node.choices {
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
            }

            let mut reachable = BTreeSet::new();
            if story.nodes.contains_key(&story.entry_node_id) {
                let mut pending = vec![story.entry_node_id.clone()];
                while let Some(node_id) = pending.pop() {
                    if !reachable.insert(node_id.clone()) {
                        continue;
                    }
                    if !story.connections.is_empty() {
                        pending.extend(
                            story
                                .connections
                                .values()
                                .filter(|connection| connection.from_node_id == node_id)
                                .map(|connection| connection.to_node_id.clone()),
                        );
                    } else if let Some(node) = story.nodes.get(&node_id) {
                        pending.extend(node.next.iter().cloned());
                        pending.extend(node.choices.iter().map(|choice| choice.next.clone()));
                    }
                }
            }
            for node_id in story.nodes.keys() {
                if !reachable.contains(node_id) {
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
