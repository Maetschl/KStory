use super::*;

#[component]
pub(crate) fn KeyRegistry(store: Signal<EditorSession>, mut key_search: Signal<String>) -> Element {
    let snapshot = store.read().store_snapshot();
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
pub(crate) fn ResourceWorkspace(
    store: Signal<EditorSession>,
    section: Section,
    mut dirty: Signal<bool>,
    mut notification: Signal<String>,
    mut notification_error: Signal<bool>,
) -> Element {
    let snapshot = store.read().store_snapshot();
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
