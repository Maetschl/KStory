use super::ConnectionDefinition;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

pub(super) fn legacy_requirements(quest_id: &str) -> Vec<RequirementDefinition> {
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
