use super::{
    content, AppError, ConnectionRequest, EditorSession, EntryViewModel, FilterState, Store,
    ACCEPTED, ON_REVIEW, WAITING,
};
#[cfg(any(target_os = "windows", target_os = "linux", target_os = "macos"))]
use dioxus::desktop::{muda, use_muda_event_handler};
use dioxus::desktop::{Config, LogicalSize, WindowBuilder};
use dioxus::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::PathBuf;

const STYLE: Asset = asset!("/assets/dioxus.css");

mod components;
mod content_studio;
mod resources;
mod translations;

use components::*;
use content_studio::*;
use resources::*;
use translations::*;

#[derive(Clone)]
struct GuiContext {
    store: EditorSession,
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

#[cfg(any(target_os = "windows", target_os = "linux", target_os = "macos"))]
fn editor_menu() -> muda::Menu {
    use muda::accelerator::{Accelerator, Code, Modifiers, CMD_OR_CTRL};
    use muda::{Menu, MenuItem, Submenu};

    let file = Submenu::new("File", true);
    let save = MenuItem::with_id(
        "kstory.save",
        "Save",
        true,
        Some(Accelerator::new(Some(CMD_OR_CTRL), Code::KeyS)),
    );
    file.append_items(&[&save]).expect("file menu can be built");

    let edit = Submenu::new("Edit", true);
    let undo = MenuItem::with_id(
        "kstory.undo",
        "Undo",
        true,
        Some(Accelerator::new(Some(CMD_OR_CTRL), Code::KeyZ)),
    );
    let redo = MenuItem::with_id(
        "kstory.redo",
        "Redo",
        true,
        Some(Accelerator::new(
            Some(CMD_OR_CTRL | Modifiers::SHIFT),
            Code::KeyZ,
        )),
    );
    edit.append_items(&[&undo, &redo])
        .expect("edit menu can be built");

    let menu = Menu::new();
    menu.append_items(&[&file, &edit])
        .expect("editor menu can be built");
    menu
}

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
    let mut config = Config::new().with_window(
        WindowBuilder::new()
            .with_title("KStory")
            .with_inner_size(LogicalSize::new(1440.0, 900.0))
            .with_min_inner_size(LogicalSize::new(1040.0, 680.0)),
    );
    #[cfg(any(target_os = "windows", target_os = "linux", target_os = "macos"))]
    {
        config = config.with_menu(editor_menu());
    }
    LaunchBuilder::desktop()
        .with_context(GuiContext {
            store: EditorSession::new(store),
        })
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
    let mut settings_open = use_signal(|| false);
    let mut notification = use_signal(|| "Catalog loaded".to_string());
    let mut notification_error = use_signal(|| false);
    let collapsed_folders = use_signal(BTreeSet::<String>::new);
    let mut explorer_width = use_signal(|| 300.0_f64);
    let mut resizing_explorer = use_signal(|| false);

    let store_snapshot = store.read().store_snapshot();
    let story_id = selected_story.read().clone();
    let story = store_snapshot.content.stories.get(&story_id).cloned();
    let locale_value = locale.read().clone();
    let selected_node_id = selected_node.read().clone();
    let selected_connection_id = selected_connection.read().clone();
    let section_value = *section.read();
    let view_value = *view.read();
    let preferences = store.read().preferences().clone();
    let can_undo = store.read().can_undo();
    let can_redo = store.read().can_redo();
    let undo_label = store.read().undo_label().unwrap_or("Undo").to_string();
    let redo_label = store.read().redo_label().unwrap_or("Redo").to_string();

    #[cfg(any(target_os = "windows", target_os = "linux", target_os = "macos"))]
    use_muda_event_handler({
        let mut store = store;
        let mut dirty = dirty;
        let mut notification = notification;
        let mut notification_error = notification_error;
        move |event| match event.id().0.as_str() {
            "kstory.undo" => {
                let result = store.write().undo();
                match result {
                    Ok(Some(label)) => {
                        dirty.set(store.read().is_dirty());
                        notification.set(format!("Undid {label}"));
                        notification_error.set(false);
                    }
                    Ok(None) => {}
                    Err(error) => {
                        notification.set(error.to_string());
                        notification_error.set(true);
                    }
                }
            }
            "kstory.redo" => {
                let result = store.write().redo();
                match result {
                    Ok(Some(label)) => {
                        dirty.set(store.read().is_dirty());
                        notification.set(format!("Redid {label}"));
                        notification_error.set(false);
                    }
                    Ok(None) => {}
                    Err(error) => {
                        notification.set(error.to_string());
                        notification_error.set(true);
                    }
                }
            }
            "kstory.save" => {
                let result = store.write().save_explicit();
                match result {
                    Ok(()) => {
                        dirty.set(false);
                        notification.set("Project saved".into());
                        notification_error.set(false);
                    }
                    Err(error) => {
                        dirty.set(store.read().is_dirty());
                        notification.set(error.to_string());
                        notification_error.set(true);
                    }
                }
            }
            _ => {}
        }
    });

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
                                        store.set(EditorSession::new(opened));
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
                            match store.write().sync_and_reset() {
                                Ok(report) => { dirty.set(false); notify(format!("Synced: {} keys, {} translations", report.created_keys, report.created_translations), false, &mut notification, &mut notification_error); }
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
                    if can_undo {
                        button {
                            class: "button button-secondary",
                            title: "{undo_label}",
                            onclick: move |_| {
                                let result = store.write().undo();
                                match result {
                                    Ok(Some(label)) => { dirty.set(store.read().is_dirty()); notify(format!("Undid {label}"), false, &mut notification, &mut notification_error); }
                                    Ok(None) => {}
                                    Err(error) => notify(error.to_string(), true, &mut notification, &mut notification_error),
                                }
                            },
                            "Undo"
                        }
                    } else {
                        button { class: "button button-secondary", disabled: true, "Undo" }
                    }
                    if can_redo {
                        button {
                            class: "button button-secondary",
                            title: "{redo_label}",
                            onclick: move |_| {
                                let result = store.write().redo();
                                match result {
                                    Ok(Some(label)) => { dirty.set(store.read().is_dirty()); notify(format!("Redid {label}"), false, &mut notification, &mut notification_error); }
                                    Ok(None) => {}
                                    Err(error) => notify(error.to_string(), true, &mut notification, &mut notification_error),
                                }
                            },
                            "Redo"
                        }
                    } else {
                        button { class: "button button-secondary", disabled: true, "Redo" }
                    }
                    if !preferences.auto_save {
                        button {
                            class: "button button-primary",
                            onclick: move |_| {
                                let result = store.write().save_explicit();
                                match result {
                                    Ok(_) => { dirty.set(false); notify("Project saved".into(), false, &mut notification, &mut notification_error); }
                                    Err(error) => { dirty.set(store.read().is_dirty()); notify(error.to_string(), true, &mut notification, &mut notification_error); }
                                }
                            },
                            if *dirty.read() { "Save *" } else { "Save" }
                        }
                    }
                    button {
                        class: "button button-secondary",
                        onclick: move |_| {
                            let open = *settings_open.read();
                            settings_open.set(!open);
                        },
                        "Settings"
                    }
                }
            }

            if *settings_open.read() {
                section { class: "settings-panel panel",
                    div { class: "panel-heading",
                        div { span { class: "eyebrow", "PREFERENCES" } h2 { "Editor settings" } }
                        button { class: "button button-secondary", onclick: move |_| settings_open.set(false), "Close" }
                    }
                    label { class: "field-label",
                        input {
                            r#type: "checkbox",
                            checked: preferences.auto_save,
                            onchange: move |event| {
                                let mut next = store.read().preferences().clone();
                                next.auto_save = event.value() == "true";
                                let result = store.write().update_preferences(next);
                                match result {
                                    Ok(_) => { dirty.set(store.read().is_dirty()); notification.set("Preferences updated".into()); notification_error.set(false); }
                                    Err(error) => { notification.set(error.to_string()); notification_error.set(true); }
                                }
                            }
                        }
                        " Auto-save changes"
                    }
                    label { class: "field-label", "History limit" }
                    input {
                        class: "field-input",
                        r#type: "number",
                        min: "1",
                        value: "{preferences.history_limit}",
                        disabled: preferences.unlimited_history,
                        onchange: move |event| {
                            let mut next = store.read().preferences().clone();
                            if let Ok(limit) = event.value().parse::<usize>() { next.history_limit = limit; }
                            match store.write().update_preferences(next) {
                                Ok(_) => { notification.set("History limit updated".into()); notification_error.set(false); }
                                Err(error) => { notification.set(error.to_string()); notification_error.set(true); }
                            }
                        }
                    }
                    label { class: "field-label",
                        input {
                            r#type: "checkbox",
                            checked: preferences.unlimited_history,
                            onchange: move |event| {
                                let mut next = store.read().preferences().clone();
                                next.unlimited_history = event.value() == "true";
                                match store.write().update_preferences(next) {
                                    Ok(_) => { notification.set("History limit updated".into()); notification_error.set(false); }
                                    Err(error) => { notification.set(error.to_string()); notification_error.set(true); }
                                }
                            }
                        }
                        " Unlimited history"
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
