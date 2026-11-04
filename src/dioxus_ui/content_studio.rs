use super::components::*;
use super::*;

#[component]
pub(crate) fn StoryExplorer(
    store: Signal<Store>,
    mut selected_story: Signal<String>,
    mut selected_node: Signal<String>,
    mut reader_node: Signal<String>,
    mut content_search: Signal<String>,
    mut collapsed_folders: Signal<BTreeSet<String>>,
    mut dirty: Signal<bool>,
    mut notification: Signal<String>,
    mut notification_error: Signal<bool>,
) -> Element {
    let snapshot = store.read().clone();
    let query = content_search.read().to_lowercase();
    let stories: Vec<String> = snapshot
        .content
        .stories
        .iter()
        .filter(|(id, story)| {
            query.is_empty()
                || id.to_lowercase().contains(&query)
                || story.name_key.to_lowercase().contains(&query)
        })
        .map(|(id, _)| id.clone())
        .collect();
    let mut folders: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for id in &stories {
        let folder = id
            .split_once('.')
            .map(|(prefix, _)| prefix.to_string())
            .unwrap_or_else(|| "standalone".into());
        folders.entry(folder).or_default().push(id.clone());
    }
    rsx! {
        aside { class: "explorer panel",
            div { class: "panel-heading",
                div { span { class: "eyebrow", "CONTENT" } h2 { "Stories" } }
                button {
                    class: "icon-button",
                    title: "Create story",
                    onclick: move |_| match store.write().create_story() {
                        Ok(created) => {
                            selected_story.set(created.story_id.clone());
                            selected_node.set(created.entry_node_id.clone());
                            reader_node.set(created.entry_node_id);
                            dirty.set(true);
                            notification.set(format!("Created {}", created.story_id));
                            notification_error.set(false);
                        }
                        Err(error) => { notification.set(error.to_string()); notification_error.set(true); }
                    },
                    "+"
                }
            }
            input {
                class: "search-input",
                placeholder: "Search stories...",
                value: "{content_search}",
                oninput: move |event| content_search.set(event.value()),
            }
            div { class: "story-list",
                for (folder, ids) in folders.iter() {
                    {
                        let folder_for_toggle = folder.clone();
                        let is_collapsed = collapsed_folders.read().contains(folder);
                        rsx! {
                            button {
                                class: "folder-row",
                                onclick: move |_| {
                                    let mut collapsed = collapsed_folders.write();
                                    if !collapsed.insert(folder_for_toggle.clone()) {
                                        collapsed.remove(&folder_for_toggle);
                                    }
                                },
                                span { class: "folder-chevron", if is_collapsed { "▸" } else { "▾" } }
                                span { class: "folder-name", "{folder_label(folder)}" }
                                span { class: "folder-count", "{ids.len()}" }
                            }
                            if !is_collapsed {
                                for id in ids.iter() {
                                    {
                                        let id = id.clone();
                                        let node_count = snapshot.content.stories.get(&id).map(|story| story.nodes.len()).unwrap_or_default();
                                        rsx! {
                                            button {
                                                class: if selected_story.read().as_str() == id { "story-row selected" } else { "story-row" },
                                                onclick: move |_| {
                                                    selected_story.set(id.clone());
                                                    if let Some(story) = store.read().content.stories.get(&id) {
                                                        selected_node.set(story.entry_node_id.clone());
                                                        reader_node.set(story.entry_node_id.clone());
                                                    }
                                                },
                                                span { class: "story-icon", "◆" }
                                                span { class: "story-name", "{id}" }
                                                span { class: "story-count", "{node_count}" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            div { class: "explorer-footer", "{stories.len()} stories" }
        }
    }
}

#[component]
pub(crate) fn StoryWorkspace(
    store: Signal<Store>,
    story: Option<content::StoryDefinition>,
    story_id: String,
    mut selected_node: Signal<String>,
    mut reader_node: Signal<String>,
    mut reader_history: Signal<Vec<String>>,
    mut selected_connection: Signal<Option<String>>,
    mut pending_connection: Signal<Option<String>>,
    mut view: Signal<StoryView>,
    view_value: StoryView,
    locale_value: String,
    mut dirty: Signal<bool>,
    mut notification: Signal<String>,
    mut notification_error: Signal<bool>,
) -> Element {
    let mut new_command_type = use_signal(|| "camera_shake_start".to_string());
    let mut new_item_id = use_signal(|| "HAMMER".to_string());
    let Some(story) = story else {
        return rsx! { section { class: "center-panel panel empty-state", "Create or select a story" } };
    };
    let (layout, issues) = content::arrange_story_top_down(&story);
    let mut nodes: Vec<(String, content::StoryNode)> = story
        .nodes
        .iter()
        .map(|(id, node)| (id.clone(), node.clone()))
        .collect();
    nodes.sort_by(|(left, _), (right, _)| {
        layout
            .get(left)
            .map(|position| {
                position.y.total_cmp(
                    &layout
                        .get(right)
                        .map(|position| position.y)
                        .unwrap_or_default(),
                )
            })
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| left.cmp(right))
    });
    let selected_node_value = selected_node.read().clone();
    let arrange_story = story.clone();
    let arrange_story_id = story_id.clone();
    let new_node_story = story.clone();
    let new_node_story_id = story_id.clone();
    let action_node_story = story.clone();
    let action_node_story_id = story_id.clone();

    rsx! {
        section { class: "center-panel panel",
            div { class: "content-toolbar",
                div { span { class: "eyebrow", "STORY" } h2 { "{story_id}" } }
                div { class: "view-switcher",
                    button { class: if view_value == StoryView::Graph { "view-button active" } else { "view-button" }, onclick: move |_| view.set(StoryView::Graph), "Graph" }
                    button { class: if view_value == StoryView::Sequence { "view-button active" } else { "view-button" }, onclick: move |_| view.set(StoryView::Sequence), "Sequence" }
                    button { class: if view_value == StoryView::Reader { "view-button active" } else { "view-button" }, onclick: move |_| view.set(StoryView::Reader), "Reader" }
                }
            }
            if view_value == StoryView::Graph {
                div { class: "graph-toolbar",
                    span { "{story.nodes.len()} nodes · {story.connections.len()} connections" }
                    if pending_connection.read().is_some() { span { class: "connection-hint", "Select an input handle to complete the connection" } }
                    button {
                        class: "button button-secondary",
                        onclick: move |_| {
                            let (positions, _) = content::arrange_story_top_down(&arrange_story);
                            if let Some(current) = store.write().content.stories.get_mut(&arrange_story_id) {
                                for (node_id, position) in positions {
                                    if let Some(node) = current.nodes.get_mut(&node_id) { node.position = [position.x, position.y]; }
                                }
                                dirty.set(true);
                                notification.set("Story arranged vertically".into());
                                notification_error.set(false);
                            }
                        },
                        "Arrange vertically"
                    }
                    button {
                        class: "button button-primary",
                        onclick: move |_| {
                            let from = selected_node.read().clone();
                            let output = new_node_story.nodes.get(&from).map(|node| {
                                if node.node_type == "choice" { format!("choice.{}", new_node_story.connections.values().filter(|connection| connection.from_node_id == from).count() + 1) } else { "default".into() }
                            });
                            let kind = new_node_story.nodes.get(&from).map(|node| if node.node_type == "choice" { "choice" } else { "next" }).unwrap_or("next");
                            match store.write().create_node(&new_node_story_id, "dialogue", "Narrator", "New dialogue line", None, if new_node_story.nodes.contains_key(&from) && new_node_story.nodes[&from].node_type != "end" { Some((from, output.unwrap_or_else(|| "default".into()), kind.into())) } else { None }) {
                                Ok(created) => { selected_node.set(created.node_id.clone()); reader_node.set(created.node_id); dirty.set(true); notification.set("Created and selected new dialogue node".into()); notification_error.set(false); }
                                Err(error) => { notification.set(error.to_string()); notification_error.set(true); }
                            }
                        },
                        "New node"
                    }
                    button {
                        class: "button button-action",
                        onclick: move |_| {
                            let from = selected_node.read().clone();
                            let can_connect = action_node_story.nodes.get(&from).is_some_and(|node| node.node_type != "end");
                            let kind = action_node_story.nodes.get(&from).map(|node| if node.node_type == "choice" { "choice" } else { "next" }).unwrap_or("next");
                            let output = if kind == "choice" {
                                Some(format!("choice.{}", action_node_story.connections.values().filter(|connection| connection.from_node_id == from).count() + 1))
                            } else if can_connect { Some("default".into()) } else { None };
                            let command_type = new_command_type.read().clone();
                            let node_type = if command_type == "show_item" { "show_item" } else if command_type == "show_icon" { "show_icon" } else { "action" };
                            let selected_item_id = new_item_id.read().clone();
                            let parameter = if command_type == "show_item" { selected_item_id.as_str() } else if command_type == "show_icon" { "LOVE" } else { command_type.as_str() };
                            match store.write().create_node(&action_node_story_id, node_type, "System", "", Some(parameter), if can_connect { Some((from, output.unwrap_or_else(|| "default".into()), kind.into())) } else { None }) {
                                Ok(created) => { selected_node.set(created.node_id.clone()); reader_node.set(created.node_id); dirty.set(true); notification.set("Created and selected action node".into()); notification_error.set(false); }
                                Err(error) => { notification.set(error.to_string()); notification_error.set(true); }
                            }
                        },
                        "New command"
                    }
                    select { class: "action-type-select", value: "{new_command_type}", onchange: move |event| new_command_type.set(event.value()),
                        option { value: "camera_shake_start", "Start camera shake" }
                        option { value: "camera_shake_stop", "Stop camera shake" }
                        option { value: "show_item", "Show item · HAMMER" }
                        option { value: "show_icon", "Show icon · LOVE" }
                    }
                    if new_command_type.read().as_str() == "show_item" {
                        select { class: "action-type-select", value: "{new_item_id}", onchange: move |event| new_item_id.set(event.value()),
                            for item_id in store.read().content.items.keys() {
                                option { value: "{item_id}", "{item_id}" }
                            }
                        }
                    }
                }
                div { class: "graph-canvas",
                    div { class: "graph-column",
                        for (id, node) in nodes.iter() {
                            {
                                let id = id.clone();
                                let node_for_text = node.clone();
                                let node_story = story.clone();
                                let node_story_id = story_id.clone();
                                let is_selected = selected_node_value == id;
                                let is_orphan = issues.orphaned_nodes.contains(&id);
                                let outgoing = content::outgoing_node_ids(&node_story, &id);
                                let event_lines: Vec<PreviewLine> = preview_lines(&store, &locale_value, node)
                                    .iter()
                                    .filter(|line| line.kind == "command" || line.kind == "event")
                                    .cloned()
                                    .collect();
                                let event_count = event_lines.len();
                                let input_id = id.clone();
                                let output_id = id.clone();
                                let node_id_for_select = id.clone();
                                rsx! {
                                    div { class: format!("node-wrap {}", if is_orphan { "node-invalid" } else { "" }),
                                        div { class: "node-handles",
                                            button { class: "handle input-handle", title: "Connect here", onclick: move |_| {
                                                let from = pending_connection.read().clone();
                                                if let Some(from) = from {
                                                    if from != input_id {
                                                        let kind = node_story.nodes.get(&from).map(|source| if source.node_type == "choice" { "choice" } else { "next" }).unwrap_or("next");
                                                        let output = if kind == "choice" { format!("choice.{}", node_story.connections.values().filter(|connection| connection.from_node_id == from).count() + 1) } else { "default".into() };
                                                        match store.write().connect_nodes(&node_story_id, ConnectionRequest { from_node_id: from, to_node_id: input_id.clone(), output_id: output, connection_kind: kind.into(), label_key: None, condition: None }) {
                                                            Ok(connection_id) => { selected_connection.set(Some(connection_id)); pending_connection.set(None); dirty.set(true); notification.set("Connection created".into()); notification_error.set(false); }
                                                            Err(error) => { notification.set(error.to_string()); notification_error.set(true); }
                                                        }
                                                    }
                                                }
                                            }, "●" }
                                            button { class: "handle output-handle", title: "Start connection", onclick: move |_| { pending_connection.set(Some(output_id.clone())); notification.set("Output selected; choose a target".into()); notification_error.set(false); }, "●" }
                                        }
                                        button { class: format!("story-node {} {}", node_type_class(&node.node_type), if is_selected { "selected" } else { "" }), onclick: move |_| { selected_node.set(node_id_for_select.clone()); reader_node.set(node_id_for_select.clone()); },
                                            span { class: "node-type", "{node.node_type}" }
                                            strong { "{id}" }
                                            span { class: "node-copy", "{node_text(&store, &locale_value, &node_for_text)}" }
                                            span { class: "node-speaker", "{node_speaker(&store, &node_for_text)}" }
                                            if event_count > 0 { span { class: "node-event-summary", "{event_count} gameplay event(s)" } }
                                            for event in event_lines.iter() {
                                                div { class: format!("node-event-line {}", command_class(&event.text)),
                                                    span { class: "node-event-kind", if event.kind == "event" { "ACTION" } else { "COMMAND" } }
                                                    span { "{event.text}" }
                                                }
                                            }
                                            if is_orphan { span { class: "node-warning", "Unreachable" } }
                                        }
                                        if !outgoing.is_empty() { div { class: "graph-arrow", "↓" } }
                                    }
                                }
                            }
                        }
                    }
                    div { class: "connection-list",
                        span { class: "eyebrow", "CONNECTIONS" }
                        for connection in story.connections.values() {
                            {
                                let connection_id = connection.id.clone();
                                let class_name = if selected_connection.read().as_deref() == Some(connection.id.as_str()) { "connection-row selected" } else { "connection-row" };
                                rsx! { button { class: class_name, onclick: move |_| selected_connection.set(Some(connection_id.clone())), "{connection.from_node_id}  →  {connection.to_node_id} · {connection.output_id}" } }
                            }
                        }
                    }
                }
            } else if view_value == StoryView::Reader {
                StoryReader { store, story, story_id, reader_node, reader_history, locale_value, notification, notification_error }
            } else {
                div { class: "sequence-view",
                    for (index, (id, node)) in nodes.iter().enumerate() {
                        {
                            let id = id.clone();
                            let lines = preview_lines(&store, &locale_value, node);
                            rsx! { button { class: if selected_node_value == id { "sequence-row selected" } else { "sequence-row" }, onclick: move |_| { selected_node.set(id.clone()); reader_node.set(id.clone()); },
                                span { class: "sequence-index", "{index + 1}" }
                                span { class: "sequence-main",
                                    strong { "{id}" }
                                    if lines.is_empty() { span { "{node_type_label(node)}" } }
                                    for line in lines.iter() {
                                        div { class: format!("sequence-line {}", if line.kind == "command" || line.kind == "event" { command_class(&line.text) } else { "sequence-text" }),
                                            if line.kind == "command" || line.kind == "event" { span { class: "sequence-line-kind", if line.kind == "event" { "ACTION" } else { "GAMEPLAY" } } }
                                            span { "{line.text}" }
                                            if line.fallback { span { class: "sequence-fallback", "Fallback · {line.status}" } }
                                        }
                                    }
                                }
                                span { class: format!("node-pill {}", node_type_class(&node.node_type)), "{node.node_type}" }
                            } }
                        }
                    }
                }
            }
            footer { class: "status-bar", "{notification}" }
        }
    }
}

#[component]
pub(crate) fn StoryReader(
    store: Signal<Store>,
    story: content::StoryDefinition,
    story_id: String,
    mut reader_node: Signal<String>,
    mut reader_history: Signal<Vec<String>>,
    locale_value: String,
    mut notification: Signal<String>,
    mut notification_error: Signal<bool>,
) -> Element {
    let current_id = reader_node.read().clone();
    let current = story
        .nodes
        .get(&current_id)
        .cloned()
        .unwrap_or_else(|| story.nodes[&story.entry_node_id].clone());
    let lines = preview_lines(&store, &locale_value, &current);
    let outgoing: Vec<content::ConnectionDefinition> = story
        .connections
        .values()
        .filter(|connection| connection.from_node_id == current_id)
        .cloned()
        .collect();
    rsx! {
        div { class: "reader-view",
            div { class: "reader-header", span { class: "eyebrow", "PREVIEW · {locale_name(&locale_value).to_uppercase()}" } h2 { "{story_id}" } span { class: "reader-note", "Simulation only · callbacks do not run" } }
            div { class: "reader-card",
                div { class: "reader-node-meta", span { class: format!("node-pill {}", node_type_class(&current.node_type)), "{current.node_type}" } span { "{current_id}" } }
                if lines.is_empty() {
                    div { class: "reader-event-block", span { class: "reader-event-kind", "{node_type_label(&current)}" } }
                }
                for line in lines.iter() {
                    if line.kind == "command" || line.kind == "event" {
                        div { class: format!("reader-command {}", command_class(&line.text)),
                            span { class: "reader-event-kind", if line.kind == "event" { "ACTION NODE" } else { "GAMEPLAY EVENT" } }
                            strong { "{line.text}" }
                            span { class: "reader-command-note", "Not translated · simulated only" }
                        }
                    } else {
                        div { class: "reader-dialogue-line",
                            div { class: "speaker-line", "{line.speaker}" }
                            p { class: "reader-text", "{line.text}" }
                            if line.fallback { div { class: "fallback-warning", "Fallback to English · {line.status}" } }
                        }
                    }
                }
                div { class: "reader-actions",
                    button { class: "button button-secondary", onclick: move |_| { reader_history.write().clear(); reader_node.set(story.entry_node_id.clone()); notification.set("Reader restarted".into()); notification_error.set(false); }, "Restart" }
                    button { class: "button button-secondary", disabled: reader_history.read().is_empty(), onclick: move |_| { if let Some(previous) = reader_history.write().pop() { reader_node.set(previous); notification.set("Reader moved back".into()); notification_error.set(false); } }, "Back" }
                    for connection in outgoing.iter() {
                        {
                            let next = connection.to_node_id.clone();
                            let previous_id = current_id.clone();
                            let label = connection.label_key.as_ref().map(|key| store.read().localized_preview(&locale_value, key).0).unwrap_or_else(|| format!("Continue to {}", connection.to_node_id));
                            rsx! { button { class: if connection.connection_kind == "choice" { "choice-button" } else { "button button-primary" }, onclick: move |_| { reader_history.write().push(previous_id.clone()); reader_node.set(next.clone()); notification.set("Reader advanced".into()); notification_error.set(false); }, "→ {label}" } }
                        }
                    }
                    if outgoing.is_empty() { span { class: "muted", "End of sequence" } }
                }
            }
        }
    }
}

#[component]
pub(crate) fn StoryInspector(
    store: Signal<Store>,
    story: Option<content::StoryDefinition>,
    story_id: String,
    selected_node_id: String,
    selected_connection_id: Option<String>,
    locale_value: String,
    mut dirty: Signal<bool>,
    mut notification: Signal<String>,
    mut notification_error: Signal<bool>,
) -> Element {
    let selected_node_value = story
        .as_ref()
        .and_then(|story| story.nodes.get(&selected_node_id))
        .cloned();
    let selected_connection_value = story
        .as_ref()
        .and_then(|story| {
            selected_connection_id
                .as_ref()
                .and_then(|id| story.connections.get(id))
        })
        .cloned();
    let selected_preview = selected_node_value
        .as_ref()
        .and_then(|node| node.dialogue_id.as_ref())
        .and_then(|dialogue_id| {
            let snapshot = store.read().clone();
            let line_id = selected_node_value
                .as_ref()
                .and_then(|node| node.dialogue_line_id.as_deref());
            let key = snapshot
                .content
                .dialogues
                .get(dialogue_id)
                .and_then(|dialogue| {
                    dialogue.lines.iter().find(|line| {
                        line.kind == "text" && line_id.map(|id| line.id == id).unwrap_or(true)
                    })
                })
                .and_then(|line| line.key.clone())?;
            Some((snapshot.localized_preview(&locale_value, &key), key))
        });
    let story_id_for_name = story_id.clone();
    let node_id_for_name = selected_node_id.clone();
    let node_type_for_name = selected_node_value
        .as_ref()
        .map(|node| node.node_type.clone())
        .unwrap_or_else(|| "dialogue".into());
    let connection_id_for_edit = selected_connection_value
        .as_ref()
        .map(|connection| connection.id.clone())
        .unwrap_or_default();
    let connection_from_for_edit = selected_connection_value
        .as_ref()
        .map(|connection| connection.from_node_id.clone())
        .unwrap_or_default();
    let connection_to_for_edit = selected_connection_value
        .as_ref()
        .map(|connection| connection.to_node_id.clone())
        .unwrap_or_default();
    let connection_output_for_edit = selected_connection_value
        .as_ref()
        .map(|connection| connection.output_id.clone())
        .unwrap_or_default();
    let connection_kind_for_edit = selected_connection_value
        .as_ref()
        .map(|connection| connection.connection_kind.clone())
        .unwrap_or_else(|| "next".into());
    let connection_label_for_edit = selected_connection_value
        .as_ref()
        .and_then(|connection| connection.label_key.clone());
    let connection_condition_for_edit = selected_connection_value
        .as_ref()
        .and_then(|connection| connection.condition.clone());
    let story_id_for_connection_target = story_id.clone();
    let story_id_for_connection_output = story_id.clone();
    let story_id_for_connection_delete = story_id.clone();
    let connection_id_for_target = connection_id_for_edit.clone();
    let connection_id_for_output = connection_id_for_edit.clone();
    let connection_id_for_delete = connection_id_for_edit.clone();
    let connection_from_for_target = connection_from_for_edit.clone();
    let connection_from_for_output = connection_from_for_edit.clone();
    let connection_output_for_target = connection_output_for_edit.clone();
    let connection_to_for_output = connection_to_for_edit.clone();
    let connection_kind_for_target = connection_kind_for_edit.clone();
    let connection_kind_for_output = connection_kind_for_edit.clone();
    let connection_label_for_target = connection_label_for_edit.clone();
    let connection_label_for_output = connection_label_for_edit.clone();
    let connection_condition_for_target = connection_condition_for_edit.clone();
    let connection_condition_for_output = connection_condition_for_edit.clone();
    rsx! {
        aside { class: "inspector panel",
            div { class: "panel-heading", div { span { class: "eyebrow", "INSPECTOR" } h2 { "Properties" } } }
            div { class: "inspector-body",
                if let Some(connection) = selected_connection_value {
                    h3 { "Connection" }
                    span { class: "node-pill node-connection", "{connection.connection_kind}" }
                    label { "Connection ID" }
                    input { class: "field-input readonly", value: "{connection.id}", readonly: true }
                    label { "From" }
                    input { class: "field-input readonly", value: "{connection.from_node_id}", readonly: true }
                    label { "To" }
                    input { class: "field-input", value: "{connection.to_node_id}", onchange: move |event| {
                        match store.write().update_connection(&story_id_for_connection_target, &connection_id_for_target, ConnectionRequest { from_node_id: connection_from_for_target.clone(), to_node_id: event.value(), output_id: connection_output_for_target.clone(), connection_kind: connection_kind_for_target.clone(), label_key: connection_label_for_target.clone(), condition: connection_condition_for_target.clone() }) {
                            Ok(_) => { dirty.set(true); notification.set("Connection target updated".into()); notification_error.set(false); }
                            Err(error) => { notification.set(error.to_string()); notification_error.set(true); }
                        }
                    } }
                    label { "Output" }
                    input { class: "field-input", value: "{connection.output_id}", onchange: move |event| {
                        match store.write().update_connection(&story_id_for_connection_output, &connection_id_for_output, ConnectionRequest { from_node_id: connection_from_for_output.clone(), to_node_id: connection_to_for_output.clone(), output_id: event.value(), connection_kind: connection_kind_for_output.clone(), label_key: connection_label_for_output.clone(), condition: connection_condition_for_output.clone() }) {
                            Ok(_) => { dirty.set(true); notification.set("Connection output updated".into()); notification_error.set(false); }
                            Err(error) => { notification.set(error.to_string()); notification_error.set(true); }
                        }
                    } }
                    button { class: "button button-danger full-width", onclick: move |_| match store.write().delete_connection(&story_id_for_connection_delete, &connection_id_for_delete) { Ok(_) => { dirty.set(true); notification.set("Connection deleted".into()); notification_error.set(false); } Err(error) => { notification.set(error.to_string()); notification_error.set(true); } }, "Disconnect" }
                } else if let Some(ref node) = selected_node_value {
                    h3 { "Node properties" }
                    span { class: format!("node-pill {}", node_type_class(&node.node_type)), "{node.node_type}" }
                    label { "Node ID" }
                    input { class: "field-input readonly", value: "{selected_node_id}", readonly: true }
                    label { "Display name key" }
                    input { class: "field-input", value: "{node.name_key}", onchange: move |event| { match store.write().update_node(&story_id_for_name, &node_id_for_name, event.value(), node_type_for_name.clone()) { Ok(_) => dirty.set(true), Err(error) => { notification.set(error.to_string()); notification_error.set(true); } } } }
                    label { "Type" }
                    input { class: "field-input readonly", value: "{node.node_type}", readonly: true }
                    if let Some(dialogue_id) = node.dialogue_id.clone() {
                        label { "Dialogue ID" }
                        input { class: "field-input readonly", value: "{dialogue_id}", readonly: true }
                    }
                    if let Some((preview, _key)) = selected_preview {
                        label { "Text · {locale_name(&locale_value)}" }
                        textarea { class: "field-textarea readonly", value: "{preview.0}", readonly: true }
                        if preview.1 { div { class: "fallback-warning", "Fallback to English · {preview.2}" } }
                    }
                    button { class: "button button-primary full-width", onclick: move |_| { dirty.set(true); notification.set("Node changes are ready to save".into()); notification_error.set(false); }, "Save node" }
                } else {
                    div { class: "empty-inspector", "Select a node or connection to inspect it." }
                }
            }
        }
    }
}

fn preview_lines(
    store: &Signal<Store>,
    locale: &str,
    node: &content::StoryNode,
) -> Vec<PreviewLine> {
    let snapshot = store.read().clone();
    if let Some(dialogue_id) = &node.dialogue_id {
        if let Some(dialogue) = snapshot.content.dialogues.get(dialogue_id) {
            let lines = dialogue
                .lines
                .iter()
                .filter(|line| {
                    node.dialogue_line_id
                        .as_deref()
                        .map(|line_id| line.id == line_id)
                        .unwrap_or(true)
                })
                .map(|line| {
                    if line.kind == "command" {
                        PreviewLine {
                            kind: "command".into(),
                            text: line
                                .command
                                .clone()
                                .unwrap_or_else(|| "[UNKNOWN COMMAND]".into()),
                            speaker: line.speaker.clone().unwrap_or_else(|| "System".into()),
                            fallback: false,
                            status: String::new(),
                        }
                    } else if let Some(key) = line.key.as_deref() {
                        let (text, fallback, status) = snapshot.localized_preview(locale, key);
                        PreviewLine {
                            kind: "text".into(),
                            text,
                            speaker: line.speaker.clone().unwrap_or_else(|| "System".into()),
                            fallback,
                            status,
                        }
                    } else {
                        PreviewLine {
                            kind: "event".into(),
                            text: line
                                .command
                                .clone()
                                .unwrap_or_else(|| "Invalid content line".into()),
                            speaker: "System".into(),
                            fallback: false,
                            status: String::new(),
                        }
                    }
                })
                .collect();
            return lines;
        }
    }

    if let Some(payload) = &node.payload {
        let text = match payload {
            content::NodePayload::ShowItem { item_id } => format!("Show item · {item_id}"),
            content::NodePayload::ShowIcon { icon_id } => format!("Show icon · {icon_id}"),
            content::NodePayload::Action(action) => {
                format!("{} · {}", action.type_name(), action_summary(action))
            }
            content::NodePayload::Condition(condition) => format!(
                "Condition · {} · {}",
                condition.condition_type,
                format_params(&condition.params)
            ),
            content::NodePayload::Event { event_id } => format!("Event · {event_id}"),
            content::NodePayload::QuestStart { quest_id } => format!("Start quest · {quest_id}"),
            content::NodePayload::QuestUpdate { quest_id } => format!("Update quest · {quest_id}"),
            content::NodePayload::SceneTransition { scene_id } => {
                format!("Change scene · {scene_id}")
            }
            content::NodePayload::Dialogue { .. } => String::new(),
            content::NodePayload::Choice => "Choice".into(),
            content::NodePayload::End => "End of story".into(),
        };
        if !text.is_empty() {
            return vec![PreviewLine {
                kind: "event".into(),
                text,
                speaker: "System".into(),
                fallback: false,
                status: String::new(),
            }];
        }
    }
    if let Some(action) = &node.action {
        return vec![PreviewLine {
            kind: "event".into(),
            text: format!("{} · {}", action.type_name(), action_summary(action)),
            speaker: "System".into(),
            fallback: false,
            status: String::new(),
        }];
    }
    if let Some(condition) = &node.condition {
        return vec![PreviewLine {
            kind: "event".into(),
            text: format!(
                "Condition · {} · {}",
                condition.condition_type,
                format_params(&condition.params)
            ),
            speaker: "System".into(),
            fallback: false,
            status: String::new(),
        }];
    }
    Vec::new()
}
