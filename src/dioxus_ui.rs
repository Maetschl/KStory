use super::{
    content, AppError, ConnectionRequest, EntryViewModel, FilterState, Store, ACCEPTED, ON_REVIEW,
    WAITING,
};
use dioxus::desktop::{Config, LogicalSize, WindowBuilder};
use dioxus::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::PathBuf;

const STYLE: Asset = asset!("/assets/dioxus.css");

#[derive(Clone)]
struct GuiContext {
    root: PathBuf,
    store: Store,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Section {
    Stories,
    Quests,
    Dialogues,
    Items,
    Entities,
    Translations,
    Keys,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum StoryView {
    Graph,
    Sequence,
    Reader,
}

#[derive(Clone)]
struct PreviewLine {
    kind: String,
    text: String,
    speaker: String,
    fallback: bool,
    status: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
struct SavedTranslationFilter {
    #[serde(default)]
    search: String,
    #[serde(default)]
    status: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
struct TranslationFilterPreferences {
    #[serde(default)]
    by_locale: BTreeMap<String, SavedTranslationFilter>,
}

const TRANSLATION_PANEL_DEFAULT_WIDTH: f64 = 340.0;
const TRANSLATION_PANEL_MIN_WIDTH: f64 = 260.0;
const TRANSLATION_PANEL_MAX_WIDTH: f64 = 520.0;

fn translation_preferences_path() -> Option<PathBuf> {
    dirs::config_dir().map(|directory| directory.join("KStory").join("translation-filters.json"))
}

fn load_translation_filter_preferences() -> TranslationFilterPreferences {
    let Some(path) = translation_preferences_path() else {
        return TranslationFilterPreferences::default();
    };
    fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

fn save_translation_filter_preferences(
    preferences: &TranslationFilterPreferences,
) -> Result<(), String> {
    let Some(path) = translation_preferences_path() else {
        return Ok(());
    };
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let text = serde_json::to_string_pretty(preferences).map_err(|error| error.to_string())?;
    fs::write(path, format!("{text}\n")).map_err(|error| error.to_string())
}

pub fn run(root: PathBuf) -> Result<(), AppError> {
    let mut store = Store::load(&root)?;
    let _ = store.sync()?;
    let config = Config::new().with_window(
        WindowBuilder::new()
            .with_title("KStory")
            .with_inner_size(LogicalSize::new(1440.0, 900.0))
            .with_min_inner_size(LogicalSize::new(1040.0, 680.0)),
    );
    LaunchBuilder::desktop()
        .with_context(GuiContext { root, store })
        .with_cfg(config)
        .launch(App);
    Ok(())
}

#[component]
fn App() -> Element {
    let context = use_context::<GuiContext>();
    let mut store = use_signal(|| context.store.clone());
    let initial_story = store
        .read()
        .content
        .stories
        .keys()
        .next()
        .cloned()
        .unwrap_or_default();
    let initial_node = store
        .read()
        .content
        .stories
        .get(&initial_story)
        .map(|story| story.entry_node_id.clone())
        .unwrap_or_default();

    let mut section = use_signal(|| Section::Stories);
    let view = use_signal(|| StoryView::Graph);
    let mut locale = use_signal(|| "es".to_string());
    let mut selected_story = use_signal(|| initial_story);
    let mut selected_node = use_signal(|| initial_node.clone());
    let mut reader_node = use_signal(|| initial_node);
    let reader_history = use_signal(Vec::<String>::new);
    let mut selected_connection = use_signal(|| None::<String>);
    let pending_connection = use_signal(|| None::<String>);
    let content_search = use_signal(String::new);
    let key_search = use_signal(String::new);
    let mut selected_key = use_signal(|| None::<String>);
    let translation_draft = use_signal(String::new);
    let translation_filters = use_signal(load_translation_filter_preferences);
    let translation_panel_width = use_signal(|| TRANSLATION_PANEL_DEFAULT_WIDTH);
    let resizing_translation_panel = use_signal(|| false);
    let mut dirty = use_signal(|| false);
    let mut notification = use_signal(|| "Catalog loaded".to_string());
    let mut notification_error = use_signal(|| false);
    let collapsed_folders = use_signal(BTreeSet::<String>::new);
    let mut explorer_width = use_signal(|| 300.0_f64);
    let mut resizing_explorer = use_signal(|| false);

    let store_snapshot = store.read().clone();
    let story_id = selected_story.read().clone();
    let story = store_snapshot.content.stories.get(&story_id).cloned();
    let locale_value = locale.read().clone();
    let selected_node_id = selected_node.read().clone();
    let selected_connection_id = selected_connection.read().clone();
    let section_value = *section.read();
    let view_value = *view.read();

    let notify = |message: String,
                  is_error: bool,
                  notification: &mut Signal<String>,
                  notification_error: &mut Signal<bool>| {
        notification.set(message);
        notification_error.set(is_error);
    };

    rsx! {
        document::Link { rel: "stylesheet", href: STYLE }
        document::Title { "KStory" }
        div { class: "app-shell",
            header { class: "topbar",
                div { class: "brand-block",
                    div { class: "brand-mark", "K" }
                    div {
                        h1 { "KStory" }
                        span { "Story authoring and localization" }
                    }
                }
                div { class: "topbar-actions",
                    label { class: "locale-label", "Locale" }
                    select {
                        value: locale_value.clone(),
                        onchange: move |event| {
                            locale.set(event.value());
                            notify("Locale changed. Reader uses accepted translations only.".into(), false, &mut notification, &mut notification_error);
                        },
                        option { value: "en", selected: locale_value == "en", "English" }
                        for target in store_snapshot.catalog.target_locales.iter() {
                            option { value: "{target}", selected: locale_value == *target, "{locale_name(target)}" }
                        }
                    }
                    span { class: "locale-active", "Active: {locale_name(&locale_value)}" }
                    button {
                        class: "button button-secondary",
                        onclick: move |_| {
                            let Some(path) = rfd::FileDialog::new()
                                .add_filter("KStory project", &["kstory"])
                                .pick_file()
                            else {
                                return;
                            };
                            match Store::load(path) {
                                Ok(mut opened) => match opened.sync() {
                                    Ok(_) => {
                                        let story_id = opened.content.stories.keys().next().cloned().unwrap_or_default();
                                        let node_id = opened.content.stories.get(&story_id).map(|story| story.entry_node_id.clone()).unwrap_or_default();
                                        store.set(opened);
                                        selected_story.set(story_id);
                                        selected_node.set(node_id.clone());
                                        reader_node.set(node_id);
                                        selected_connection.set(None);
                                        selected_key.set(None);
                                        notify("Project opened.".into(), false, &mut notification, &mut notification_error);
                                    }
                                    Err(error) => notify(error.to_string(), true, &mut notification, &mut notification_error),
                                },
                                Err(error) => notify(error.to_string(), true, &mut notification, &mut notification_error),
                            }
                        },
                        "Open project"
                    }
                    button {
                        class: "button button-secondary",
                        onclick: move |_| {
                            match store.write().sync() {
                                Ok(report) => notify(format!("Synced: {} keys, {} translations", report.created_keys, report.created_translations), false, &mut notification, &mut notification_error),
                                Err(error) => notify(error.to_string(), true, &mut notification, &mut notification_error),
                            }
                        },
                        "Sync"
                    }
                    button {
                        class: "button button-secondary",
                        onclick: move |_| {
                            match store.read().validate() {
                                Ok(_) => notify("Catalog validation passed".into(), false, &mut notification, &mut notification_error),
                                Err(error) => notify(error.to_string(), true, &mut notification, &mut notification_error),
                            }
                        },
                        "Validate"
                    }
                    button {
                        class: "button button-primary",
                        onclick: move |_| {
                            let result = store.read().save_content();
                            match result {
                                Ok(_) => { dirty.set(false); notify("Content saved".into(), false, &mut notification, &mut notification_error); }
                                Err(error) => notify(error.to_string(), true, &mut notification, &mut notification_error),
                            }
                        },
                        if *dirty.read() { "Save *" } else { "Save" }
                    }
                }
            }

            nav { class: "module-tabs",
                for (value, label) in [
                    (Section::Stories, "Stories"),
                    (Section::Quests, "Quests"),
                    (Section::Dialogues, "Dialogues"),
                    (Section::Items, "Items"),
                    (Section::Entities, "Entities"),
                    (Section::Translations, "Translations"),
                    (Section::Keys, "Keys"),
                ] {
                    button {
                        class: if section_value == value { "module-tab active" } else { "module-tab" },
                        onclick: move |_| section.set(value),
                        "{label}"
                    }
                }
                div { class: "module-spacer" }
                span { class: if *notification_error.read() { "save-state error" } else { "save-state" }, "{notification}" }
            }

            if section_value == Section::Stories {
                div {
                    class: "workspace",
                    style: "grid-template-columns: {explorer_width}px 8px minmax(520px, 1fr) 320px;",
                    onpointermove: move |event| {
                        if *resizing_explorer.read() {
                            let x = event.client_coordinates().x;
                            explorer_width.set(x.clamp(240.0, 480.0));
                        }
                    },
                    onpointerup: move |_| resizing_explorer.set(false),
                    onpointerleave: move |_| resizing_explorer.set(false),
                    StoryExplorer {
                        store,
                        selected_story,
                        selected_node,
                        reader_node,
                        content_search,
                        collapsed_folders,
                        dirty,
                        notification,
                        notification_error,
                    }
                    div {
                        class: if *resizing_explorer.read() { "panel-resizer active" } else { "panel-resizer" },
                        role: "separator",
                        aria_label: "Resize story explorer",
                        onpointerdown: move |event| {
                            event.prevent_default();
                            resizing_explorer.set(true);
                        },
                        title: "Drag to resize the story explorer",
                    }
                    StoryWorkspace {
                        store,
                        story: story.clone(),
                        story_id: story_id.clone(),
                        selected_node,
                        reader_node,
                        reader_history,
                        selected_connection,
                        pending_connection,
                        view,
                        view_value,
                        locale_value: locale_value.clone(),
                        dirty,
                        notification,
                        notification_error,
                    }
                    StoryInspector {
                        store,
                        story: story.clone(),
                        story_id,
                        selected_node_id,
                        selected_connection_id,
                        locale_value: locale_value.clone(),
                        dirty,
                        notification,
                        notification_error,
                    }
                }
            } else if section_value == Section::Translations {
                    TranslationWorkspace {
                    store,
                        locale_value: locale_value.clone(),
                    selected_key,
                    translation_draft,
                        translation_filters,
                        translation_panel_width,
                        resizing_translation_panel,
                    dirty,
                    notification,
                    notification_error,
                }
            } else if section_value == Section::Keys {
                KeyRegistry { store, key_search }
            } else {
                ResourceWorkspace { store, section: section_value, dirty, notification, notification_error }
            }
        }
    }
}

#[component]
fn StoryExplorer(
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
fn StoryWorkspace(
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
fn StoryReader(
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
fn StoryInspector(
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

#[component]
fn TranslationWorkspace(
    store: Signal<Store>,
    locale_value: String,
    mut selected_key: Signal<Option<String>>,
    mut translation_draft: Signal<String>,
    mut translation_filters: Signal<TranslationFilterPreferences>,
    mut translation_panel_width: Signal<f64>,
    mut resizing_translation_panel: Signal<bool>,
    mut dirty: Signal<bool>,
    mut notification: Signal<String>,
    mut notification_error: Signal<bool>,
) -> Element {
    let snapshot = store.read().clone();
    let saved_filter = translation_filters
        .read()
        .by_locale
        .get(&locale_value)
        .cloned()
        .unwrap_or_default();
    let filters = FilterState {
        search: saved_filter.search.clone(),
        status: saved_filter.status.clone(),
        ..Default::default()
    };
    let keys: Vec<String> = snapshot.filtered_keys(&locale_value, &filters);
    let status_counts = snapshot.status_counts(&locale_value);
    let status_options: Vec<(String, String)> = if locale_value == snapshot.catalog.source_locale {
        vec![
            (String::new(), "All".into()),
            ("source".into(), "Source".into()),
        ]
    } else {
        vec![
            (String::new(), "All".into()),
            (WAITING.into(), "Waiting for translation".into()),
            (ON_REVIEW.into(), "On review".into()),
            (ACCEPTED.into(), "Accepted".into()),
        ]
    };
    let selected = selected_key.read().clone();
    let view = selected
        .as_ref()
        .and_then(|key| snapshot.entry_view(&locale_value, key));
    let selected_is_filtered_out = selected
        .as_ref()
        .map(|key| !keys.iter().any(|visible_key| visible_key == key))
        .unwrap_or(false);
    let locale_for_search = locale_value.clone();
    let locale_for_clear = locale_value.clone();
    let locale_for_status = locale_value.clone();
    let mut translation_filters_for_search = translation_filters;
    let mut translation_filters_for_clear = translation_filters;
    let mut translation_filters_for_status = translation_filters;
    let mut notification_for_search = notification;
    let mut notification_error_for_search = notification_error;
    let mut notification_for_clear = notification;
    let mut notification_error_for_clear = notification_error;
    let mut notification_for_status = notification;
    let mut notification_error_for_status = notification_error;
    rsx! {
        div {
            class: "translation-workspace",
            style: "grid-template-columns: {translation_panel_width}px 8px minmax(520px, 1fr);",
            onpointermove: move |event| {
                if *resizing_translation_panel.read() {
                    let width = clamp_translation_panel_width(event.client_coordinates().x);
                    translation_panel_width.set(width);
                }
            },
            onpointerup: move |_| resizing_translation_panel.set(false),
            onpointerleave: move |_| resizing_translation_panel.set(false),
            aside { class: "translation-list panel",
                div { class: "panel-heading", div { span { class: "eyebrow", "CATALOG" } h2 { "Translations" } } }
                input {
                    class: "search-input",
                    placeholder: "Search key, source, translation, story, quest, owner, speaker or type...",
                    value: "{saved_filter.search}",
                    oninput: move |event| {
                        let value = event.value();
                        let mut preferences = translation_filters_for_search.read().clone();
                        preferences
                            .by_locale
                            .entry(locale_for_search.clone())
                            .or_default()
                            .search = value;
                        match save_translation_filter_preferences(&preferences) {
                            Ok(_) => translation_filters_for_search.set(preferences),
                            Err(error) => {
                                notification_for_search.set(format!("Could not save filter preferences: {error}"));
                                notification_error_for_search.set(true);
                            }
                        }
                    }
                }
                div { class: "translation-filter-toolbar",
                    div { class: "translation-filter-chips",
                        for (status_value, status_label) in status_options.iter() {
                            {
                                let status_value = status_value.clone();
                                let status_label = status_label.clone();
                                let active = saved_filter.status == status_value;
                                let count = if status_value.is_empty() {
                                    status_counts.values().sum()
                                } else {
                                    status_counts.get(status_value.as_str()).copied().unwrap_or_default()
                                };
                                let chip_class = translation_filter_class(&status_value, active);
                                let locale_for_chip = locale_for_status.clone();
                                rsx! {
                                    button {
                                        class: "{chip_class}",
                                        title: "Filter translations by status",
                                        onclick: move |_| {
                                            let mut preferences = translation_filters_for_status.read().clone();
                                            preferences
                                                .by_locale
                                                .entry(locale_for_chip.clone())
                                                .or_default()
                                                .status = status_value.clone();
                                            match save_translation_filter_preferences(&preferences) {
                                                Ok(_) => translation_filters_for_status.set(preferences),
                                                Err(error) => {
                                                    notification_for_status.set(format!("Could not save filter preferences: {error}"));
                                                    notification_error_for_status.set(true);
                                                }
                                            }
                                        },
                                        "{status_label} · {count}"
                                    }
                                }
                            }
                        }
                    }
                    button {
                        class: "button button-secondary translation-clear",
                        disabled: saved_filter.search.is_empty() && saved_filter.status.is_empty(),
                        onclick: move |_| {
                            let mut preferences = translation_filters_for_clear.read().clone();
                            preferences.by_locale.remove(&locale_for_clear);
                            match save_translation_filter_preferences(&preferences) {
                                Ok(_) => {
                                    translation_filters_for_clear.set(preferences);
                                }
                                Err(error) => {
                                    notification_for_clear.set(format!("Could not save filter preferences: {error}"));
                                    notification_error_for_clear.set(true);
                                }
                            }
                        },
                        "Clear"
                    }
                }
                div { class: "translation-result-summary",
                    "{keys.len()} results",
                    if selected_is_filtered_out { " · Selected entry is outside the current filter" }
                }
                div { class: "story-list",
                    for key in keys.iter() {
                        {
                            let key = key.clone();
                            let locale_for_key = locale_value.clone();
                            let key_for_click = key.clone();
                            let status = snapshot.effective_translation_status(&locale_value, &key);
                            let status_class = translation_status_class(status);
                            let status_label = translation_status_label(status);
                            rsx! {
                                button {
                                    class: if selected.as_deref() == Some(key.as_str()) { "story-row selected" } else { "story-row" },
                                    onclick: move |_| {
                                        let text = store.read().entry_view(&locale_for_key, &key_for_click).map(|view| view.target_text).unwrap_or_default();
                                        selected_key.set(Some(key_for_click.clone()));
                                        translation_draft.set(text);
                                    },
                                    span { class: "story-icon", "□" }
                                    span { class: "story-name", "{key}" }
                                    span { class: "{status_class}", "{status_label}" }
                                }
                            }
                        }
                    }
                }
            }
            div {
                class: if *resizing_translation_panel.read() { "panel-resizer active" } else { "panel-resizer" },
                role: "separator",
                aria_label: "Resize translation list",
                title: "Drag to resize. Double-click to reset.",
                onpointerdown: move |event| {
                    event.prevent_default();
                    resizing_translation_panel.set(true);
                },
                ondoubleclick: move |_| translation_panel_width.set(TRANSLATION_PANEL_DEFAULT_WIDTH),
            }
            section { class: "translation-editor panel",
                if let Some(view) = view {
                    TranslationEditor { store, view, locale_value, translation_draft, dirty, notification, notification_error }
                } else { div { class: "empty-state", "Select a translation key" } }
            }
        }
    }
}

fn translation_filter_class(status: &str, active: bool) -> &'static str {
    match (status, active) {
        (WAITING, true) => "filter-chip waiting active",
        (ON_REVIEW, true) => "filter-chip review active",
        (ACCEPTED, true) => "filter-chip accepted active",
        ("source", true) => "filter-chip source active",
        (_, true) => "filter-chip active",
        (WAITING, false) => "filter-chip waiting",
        (ON_REVIEW, false) => "filter-chip review",
        (ACCEPTED, false) => "filter-chip accepted",
        ("source", false) => "filter-chip source",
        (_, false) => "filter-chip",
    }
}

fn clamp_translation_panel_width(width: f64) -> f64 {
    width.clamp(TRANSLATION_PANEL_MIN_WIDTH, TRANSLATION_PANEL_MAX_WIDTH)
}

fn translation_status_class(status: &str) -> &'static str {
    match status {
        WAITING => "translation-status waiting",
        ON_REVIEW => "translation-status review",
        ACCEPTED => "translation-status accepted",
        "source" => "translation-status source",
        _ => "translation-status",
    }
}

fn translation_status_label(status: &str) -> &'static str {
    match status {
        WAITING => "Waiting",
        ON_REVIEW => "On review",
        ACCEPTED => "Accepted",
        "source" => "Source",
        _ => "Unknown",
    }
}

#[component]
fn TranslationEditor(
    store: Signal<Store>,
    view: EntryViewModel,
    locale_value: String,
    mut translation_draft: Signal<String>,
    mut dirty: Signal<bool>,
    mut notification: Signal<String>,
    mut notification_error: Signal<bool>,
) -> Element {
    let key = view.key.clone();
    let locale_for_save = locale_value.clone();
    let locale_for_review = locale_value.clone();
    let locale_for_accept = locale_value.clone();
    let key_for_save = key.clone();
    let key_for_review = key.clone();
    let key_for_accept = key.clone();
    rsx! {
        div { class: "editor-body",
            span { class: "eyebrow", "TRANSLATION" }
            h2 { "{key}" }
            div { class: "status-line", "Status: {view.status}" }
            label { "English source" }
            textarea { class: "field-textarea source-field", value: "{view.source_text}", readonly: true }
            label { "{locale_name(&locale_value)}" }
            textarea { class: "field-textarea", value: "{translation_draft}", oninput: move |event| { translation_draft.set(event.value()); dirty.set(true); } }
            if !view.validation_errors.is_empty() { div { class: "validation-errors", for error in view.validation_errors.iter() { div { "! {error}" } } } }
            div { class: "reader-actions",
                button { class: "button button-primary", onclick: move |_| match store.write().save_translation(&locale_for_save, &key_for_save, translation_draft.read().clone()) { Ok(_) => { dirty.set(false); notification.set("Translation saved and waiting for review".into()); notification_error.set(false); } Err(error) => { notification.set(error.to_string()); notification_error.set(true); } }, "Save" }
                button { class: "button button-secondary", onclick: move |_| match store.write().mark_on_review(&locale_for_review, &key_for_review) { Ok(_) => { notification.set("Translation marked on review".into()); notification_error.set(false); } Err(error) => { notification.set(error.to_string()); notification_error.set(true); } }, "Mark on review" }
                button { class: "button button-success", disabled: view.status != ON_REVIEW, onclick: move |_| match store.write().accept(&locale_for_accept, &key_for_accept) { Ok(_) => { notification.set("Translation accepted".into()); notification_error.set(false); } Err(error) => { notification.set(error.to_string()); notification_error.set(true); } }, "Accept" }
            }
        }
    }
}

#[component]
fn KeyRegistry(store: Signal<Store>, mut key_search: Signal<String>) -> Element {
    let snapshot = store.read().clone();
    let query = key_search.read().to_lowercase();
    let keys: Vec<(String, String)> = snapshot
        .catalog
        .entries
        .iter()
        .filter(|(key, metadata)| {
            query.is_empty()
                || key.to_lowercase().contains(&query)
                || metadata.entry_type.to_lowercase().contains(&query)
        })
        .map(|(key, metadata)| (key.clone(), metadata.entry_type.clone()))
        .collect();
    rsx! {
        div { class: "resource-workspace",
            section { class: "resource-panel panel",
                div { class: "panel-heading", div { span { class: "eyebrow", "REGISTRY" } h2 { "Key registry" } } }
                input { class: "search-input wide-search", placeholder: "Search keys...", value: "{key_search}", oninput: move |event| key_search.set(event.value()) }
                div { class: "resource-grid",
                    for (key, entry_type) in keys.iter() { div { class: "resource-card", strong { "{key}" } span { "{entry_type}" } } }
                }
            }
        }
    }
}

#[component]
fn ResourceWorkspace(
    store: Signal<Store>,
    section: Section,
    mut dirty: Signal<bool>,
    mut notification: Signal<String>,
    mut notification_error: Signal<bool>,
) -> Element {
    let snapshot = store.read().clone();
    let (title, entries): (String, Vec<String>) = match section {
        Section::Quests => (
            "Quests".into(),
            snapshot.content.quests.keys().cloned().collect(),
        ),
        Section::Dialogues => (
            "Dialogues".into(),
            snapshot.content.dialogues.keys().cloned().collect(),
        ),
        Section::Items => (
            "Items".into(),
            snapshot.content.items.keys().cloned().collect(),
        ),
        Section::Entities => (
            "Entities".into(),
            snapshot.content.entities.keys().cloned().collect(),
        ),
        _ => ("Content".into(), Vec::new()),
    };
    let title_for_button = title.clone();
    rsx! {
        div { class: "resource-workspace",
            section { class: "resource-panel panel",
                div { class: "panel-heading", div { span { class: "eyebrow", "CONTENT" } h2 { "{title}" } } button { class: "button button-primary", onclick: move |_| { dirty.set(true); notification.set(format!("New {title_for_button} is ready to configure")); notification_error.set(false); }, "New" } }
                div { class: "resource-grid",
                    for id in entries.iter() { div { class: "resource-card", strong { "{id}" } span { "Stable ID · select to edit" } } }
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

fn action_summary(action: &content::ActionDefinition) -> String {
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

fn format_params(params: &std::collections::BTreeMap<String, serde_json::Value>) -> String {
    if params.is_empty() {
        return "no parameters".into();
    }
    params
        .iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn command_class(command: &str) -> &'static str {
    let normalized = command.to_ascii_uppercase();
    if normalized.starts_with("[SHOW:") {
        "command-item"
    } else if normalized.contains("CAMERA_SHAKE") || normalized.contains("ACTIVE_BUTTON") {
        "command-action"
    } else {
        "command-system"
    }
}

fn node_type_label(node: &content::StoryNode) -> String {
    if let Some(action) = &node.action {
        return format!("Action · {}", action.type_name());
    }
    if let Some(condition) = &node.condition {
        return format!("Condition · {}", condition.condition_type);
    }
    node.node_type.replace('_', " ")
}

fn node_type_class(node_type: &str) -> &'static str {
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

fn locale_name(locale: &str) -> &'static str {
    match locale {
        "es" => "Español",
        "en" => "English",
        _ => "Locale",
    }
}

fn folder_label(folder: &str) -> String {
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

fn node_speaker(store: &Signal<Store>, node: &content::StoryNode) -> String {
    let snapshot = store.read().clone();
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

fn node_text(store: &Signal<Store>, locale: &str, node: &content::StoryNode) -> String {
    let Some(dialogue_id) = &node.dialogue_id else {
        return node.node_type.replace('_', " ");
    };
    let snapshot = store.read().clone();
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translation_filter_preferences_round_trip_by_locale() {
        let preferences = TranslationFilterPreferences {
            by_locale: BTreeMap::from([
                (
                    "es".into(),
                    SavedTranslationFilter {
                        search: "item.".into(),
                        status: WAITING.into(),
                    },
                ),
                (
                    "fr".into(),
                    SavedTranslationFilter {
                        search: "dialog".into(),
                        status: ON_REVIEW.into(),
                    },
                ),
            ]),
        };
        let encoded = serde_json::to_string(&preferences).unwrap();
        let decoded: TranslationFilterPreferences = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded, preferences);
        assert_eq!(decoded.by_locale["es"].status, WAITING);
        assert_eq!(decoded.by_locale["fr"].search, "dialog");
    }

    #[test]
    fn translation_filter_classes_keep_status_colors() {
        assert_eq!(
            translation_filter_class(WAITING, true),
            "filter-chip waiting active"
        );
        assert_eq!(
            translation_filter_class(ON_REVIEW, false),
            "filter-chip review"
        );
        assert_eq!(
            translation_filter_class(ACCEPTED, true),
            "filter-chip accepted active"
        );
    }

    #[test]
    fn translation_panel_width_stays_within_layout_bounds() {
        assert_eq!(
            clamp_translation_panel_width(100.0),
            TRANSLATION_PANEL_MIN_WIDTH
        );
        assert_eq!(
            clamp_translation_panel_width(TRANSLATION_PANEL_DEFAULT_WIDTH),
            TRANSLATION_PANEL_DEFAULT_WIDTH
        );
        assert_eq!(
            clamp_translation_panel_width(900.0),
            TRANSLATION_PANEL_MAX_WIDTH
        );
    }
}
