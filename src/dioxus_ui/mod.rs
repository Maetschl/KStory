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
        .with_context(GuiContext { store })
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
