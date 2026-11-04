use super::EditorSession;
use super::*;

pub(crate) fn action_summary(action: &content::ActionDefinition) -> String {
    match action {
        content::ActionDefinition::StartDialogue { dialogue_id } => {
            format!("dialogue_id={dialogue_id}")
        }
        content::ActionDefinition::StartQuest { quest_id }
        | content::ActionDefinition::CompleteQuest { quest_id } => format!("quest_id={quest_id}"),
        content::ActionDefinition::GiveItem { item_id, amount }
        | content::ActionDefinition::RemoveItem { item_id, amount } => {
            format!("item_id={item_id}, amount={amount}")
        }
        content::ActionDefinition::SetFlag { flag }
        | content::ActionDefinition::ClearFlag { flag } => format!("flag={flag}"),
        content::ActionDefinition::TriggerEvent { event_id } => format!("event_id={event_id}"),
        content::ActionDefinition::ChangeScene { scene_id } => format!("scene_id={scene_id}"),
        content::ActionDefinition::SpawnEntity { entity_id }
        | content::ActionDefinition::DespawnEntity { entity_id } => {
            format!("entity_id={entity_id}")
        }
        content::ActionDefinition::SetEntityState { entity_id, state } => {
            format!("entity_id={entity_id}, state={state}")
        }
        content::ActionDefinition::EnableGameplayButton { button_id } => {
            format!("button_id={button_id}")
        }
        content::ActionDefinition::Callback { callback_id, .. } => {
            format!("callback_id={callback_id}")
        }
        content::ActionDefinition::CameraShakeStart
        | content::ActionDefinition::CameraShakeStop => "no parameters".into(),
        content::ActionDefinition::LegacyShowItem { item_id } => format!("item_id={item_id}"),
        content::ActionDefinition::LegacyShowIcon { icon_id } => format!("icon_id={icon_id}"),
    }
}

pub(crate) fn format_params(
    params: &std::collections::BTreeMap<String, serde_json::Value>,
) -> String {
    if params.is_empty() {
        return "no parameters".into();
    }
    params
        .iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>()
        .join(", ")
}

pub(crate) fn command_class(command: &str) -> &'static str {
    let normalized = command.to_ascii_uppercase();
    if normalized.starts_with("[SHOW:") {
        "command-item"
    } else if normalized.contains("CAMERA_SHAKE") || normalized.contains("ACTIVE_BUTTON") {
        "command-action"
    } else {
        "command-system"
    }
}

pub(crate) fn node_type_label(node: &content::StoryNode) -> String {
    if let Some(action) = &node.action {
        return format!("Action · {}", action.type_name());
    }
    if let Some(condition) = &node.condition {
        return format!("Condition · {}", condition.condition_type);
    }
    node.node_type.replace('_', " ")
}

pub(crate) fn node_type_class(node_type: &str) -> &'static str {
    match node_type {
        "dialogue" => "node-dialogue",
        "show_item" => "node-show-item",
        "show_icon" => "node-show-icon",
        "choice" => "node-choice",
        "condition" => "node-condition",
        "action" => "node-action",
        "quest_start" | "quest_update" => "node-quest",
        "scene_transition" => "node-transition",
        "event" => "node-event",
        "end" => "node-end",
        _ => "node-default",
    }
}

pub(crate) fn locale_name(locale: &str) -> &'static str {
    match locale {
        "es" => "Español",
        "en" => "English",
        _ => "Locale",
    }
}

pub(crate) fn folder_label(folder: &str) -> String {
    if folder == "standalone" {
        return "Standalone".into();
    }
    folder
        .split('_')
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

pub(crate) fn node_speaker(store: &Signal<EditorSession>, node: &content::StoryNode) -> String {
    let snapshot = store.read().store_snapshot();
    let line_id = node.dialogue_line_id.as_deref();
    node.dialogue_id
        .as_ref()
        .and_then(|id| snapshot.content.dialogues.get(id))
        .and_then(|dialogue| {
            dialogue
                .lines
                .iter()
                .find(|line| line.kind == "text" && line_id.map(|id| line.id == id).unwrap_or(true))
        })
        .and_then(|line| line.speaker.clone())
        .unwrap_or_else(|| "System".into())
}

pub(crate) fn node_text(
    store: &Signal<EditorSession>,
    locale: &str,
    node: &content::StoryNode,
) -> String {
    let Some(dialogue_id) = &node.dialogue_id else {
        return node.node_type.replace('_', " ");
    };
    let snapshot = store.read().store_snapshot();
    let Some(dialogue) = snapshot.content.dialogues.get(dialogue_id) else {
        return "Dialogue missing".into();
    };
    let line_id = node.dialogue_line_id.as_deref();
    let Some(key) = dialogue
        .lines
        .iter()
        .find(|line| line.kind == "text" && line_id.map(|id| line.id == id).unwrap_or(true))
        .and_then(|line| line.key.as_deref())
    else {
        return "Text missing".into();
    };
    snapshot.localized_preview(locale, key).0
}
