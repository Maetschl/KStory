use super::components::*;
use super::*;

#[component]
pub(crate) fn TranslationWorkspace(
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

pub(super) fn translation_filter_class(status: &str, active: bool) -> &'static str {
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

pub(super) fn clamp_translation_panel_width(width: f64) -> f64 {
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
pub(crate) fn TranslationEditor(
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
