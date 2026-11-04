use super::*;
use eframe::egui;
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub(crate) struct UiNotification {
    message: String,
    is_error: bool,
    expires_at: Instant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EditorAction {
    Save,
    MarkOnReview,
    Accept,
    Reject,
    Reset,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ContextAction {
    MarkWaiting,
    MarkOnReview,
    Accept,
    Reject,
    ShowUsages,
    ShowMetadata,
}

fn run_gui(root: PathBuf) -> Result<(), AppError> {
    dioxus_ui::run(root)
}

fn configure_theme(context: &egui::Context) {
    let mut style = (*context.style()).clone();
    style.spacing.item_spacing = egui::vec2(8.0, 6.0);
    style.spacing.button_padding = egui::vec2(10.0, 6.0);
    style.visuals.window_fill = egui::Color32::from_rgb(27, 31, 38);
    style.visuals.panel_fill = egui::Color32::from_rgb(22, 26, 32);
    style.visuals.faint_bg_color = egui::Color32::from_rgb(33, 38, 47);
    style.visuals.extreme_bg_color = egui::Color32::from_rgb(15, 18, 22);
    style.visuals.override_text_color = Some(egui::Color32::from_rgb(226, 232, 240));
    style.visuals.widgets.inactive.bg_fill = egui::Color32::from_rgb(52, 62, 76);
    style.visuals.widgets.inactive.fg_stroke.color = egui::Color32::from_rgb(218, 226, 238);
    style.visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(67, 83, 103);
    style.visuals.widgets.hovered.fg_stroke.color = egui::Color32::WHITE;
    style.visuals.widgets.active.bg_fill = egui::Color32::from_rgb(48, 106, 160);
    style.visuals.widgets.active.fg_stroke.color = egui::Color32::WHITE;
    style.visuals.selection.bg_fill = egui::Color32::from_rgb(45, 96, 145);
    style.spacing.scroll = egui::style::ScrollStyle {
        floating: false,
        bar_width: 10.0,
        handle_min_length: 32.0,
        bar_inner_margin: 3.0,
        bar_outer_margin: 2.0,
        foreground_color: true,
        ..egui::style::ScrollStyle::solid()
    };
    context.set_style(style);
}

fn app_background() -> egui::Color32 {
    egui::Color32::from_rgb(24, 30, 38)
}

fn panel_background() -> egui::Color32 {
    egui::Color32::from_rgb(27, 33, 42)
}

fn toolbar_background() -> egui::Color32 {
    egui::Color32::from_rgb(34, 41, 51)
}

fn panel_frame(fill: egui::Color32, margin: f32) -> egui::Frame {
    egui::Frame::default()
        .fill(fill)
        .inner_margin(egui::Margin::same(margin))
}

/// Keep a side panel's inner UI from growing the panel because of a long key,
/// story id, or an unbounded horizontal row. `SidePanel::exact_width` controls
/// the requested panel size, but egui can still expand the stored panel rect
/// when a child reports a larger natural width. All side-panel content must be
/// bounded before adding widgets.
fn constrain_side_panel_content(ui: &mut egui::Ui) {
    // The panel width is now user-resizable. Lock the contents to the width
    // selected by the user so long IDs/text cannot resize it a second time.
    let available_width = ui.available_width().max(0.0);
    ui.set_width(available_width);
}

fn toolbar_row(ui: &mut egui::Ui, height: f32, add_contents: impl FnOnce(&mut egui::Ui)) {
    let width = ui.available_width().max(0.0);
    ui.allocate_ui_with_layout(
        egui::vec2(width, height),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            add_contents(ui);
        },
    );
}

fn status_color(status: &str) -> egui::Color32 {
    match status {
        WAITING => egui::Color32::from_rgb(236, 173, 72),
        ON_REVIEW => egui::Color32::from_rgb(83, 169, 230),
        ACCEPTED => egui::Color32::from_rgb(92, 195, 124),
        "source" => egui::Color32::from_rgb(160, 180, 205),
        _ => egui::Color32::LIGHT_GRAY,
    }
}

fn status_label(ui: &mut egui::Ui, status: &str) {
    ui.colored_label(
        status_color(status),
        format!("● {}", status_display(status)),
    );
}

fn status_display(status: &str) -> String {
    status.replace('_', " ")
}

pub(crate) struct EditorApp {
    pub(crate) root: PathBuf,
    pub(crate) store: Store,
    pub(crate) locale: String,
    pub(crate) filters: FilterState,
    pub(crate) show_content: bool,
    pub(crate) show_studio: bool,
    pub(crate) show_inspector: bool,
    pub(crate) show_metadata: bool,
    pub(crate) selected: Option<String>,
    pub(crate) draft: String,
    pub(crate) draft_dirty: bool,
    pub(crate) notification: Option<UiNotification>,
    pub(crate) external_change: std::sync::mpsc::Receiver<bool>,
    pub(crate) external_snapshot: Option<Store>,
    pub(crate) external_conflict: bool,
    pub(crate) show_compare: bool,
    pub(crate) confirm_reset: bool,
    pub(crate) selected_story_id: Option<String>,
    pub(crate) selected_node_id: Option<String>,
    pub(crate) content_search: String,
    pub(crate) content_view: ContentView,
    pub(crate) content_dirty: bool,
    pub(crate) content_section: ContentSection,
    pub(crate) selected_content_id: Option<String>,
    pub(crate) reader: StoryReaderState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ContentSection {
    Stories,
    Quests,
    Dialogues,
    Items,
    Entities,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ContentView {
    Graph,
    Sequence,
    Reader,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct StoryReaderState {
    pub(crate) story_id: Option<String>,
    pub(crate) current_node_id: Option<String>,
    pub(crate) history: Vec<String>,
    pub(crate) selected_choice: Option<String>,
    pub(crate) locale: String,
    pub(crate) active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LocalizedPreview {
    pub(crate) text: String,
    pub(crate) used_fallback: bool,
    pub(crate) status: String,
}

impl StoryReaderState {
    pub(crate) fn restart_at(&mut self, story_id: String, entry_node_id: String, locale: String) {
        self.story_id = Some(story_id);
        self.current_node_id = Some(entry_node_id);
        self.history.clear();
        self.selected_choice = None;
        self.locale = locale;
        self.active = true;
    }

    pub(crate) fn back(&mut self) {
        if let Some(previous) = self.history.pop() {
            self.current_node_id = Some(previous);
            self.selected_choice = None;
        }
    }

    pub(crate) fn advance_to(&mut self, next: String) {
        if let Some(current) = self.current_node_id.take() {
            self.history.push(current);
        }
        self.current_node_id = Some(next);
        self.selected_choice = None;
    }
}

impl EditorApp {
    pub(crate) fn request_selection(&mut self, key: String) {
        if self.selected.as_deref() == Some(key.as_str()) {
            return;
        }
        if self.draft_dirty {
            self.notify(
                "Save or reset the current draft before changing selection.",
                true,
            );
            return;
        }
        self.selected = Some(key);
        self.load_draft();
    }

    pub(crate) fn load_draft(&mut self) {
        self.draft = self
            .selected
            .as_deref()
            .and_then(|key| {
                if self.locale == self.store.catalog.source_locale {
                    self.store.source.entries.get(key)
                } else {
                    self.store
                        .targets
                        .get(&self.locale)
                        .and_then(|file| file.entries.get(key))
                }
            })
            .map(|entry| entry.text.clone())
            .unwrap_or_default();
        self.draft_dirty = false;
    }

    fn notify(&mut self, message: impl Into<String>, is_error: bool) {
        self.notification = Some(UiNotification {
            message: message.into(),
            is_error,
            expires_at: Instant::now() + Duration::from_secs(6),
        });
    }

    fn selected_view(&self) -> Option<EntryViewModel> {
        self.selected
            .as_deref()
            .and_then(|key| self.store.entry_view(&self.locale, key))
    }

    fn target_status(&self, key: &str) -> &str {
        if self.locale == self.store.catalog.source_locale {
            return "source";
        }
        self.store
            .targets
            .get(&self.locale)
            .and_then(|file| file.entries.get(key))
            .map(|entry| entry.status.as_str())
            .unwrap_or(WAITING)
    }

    fn values_for(&self, field: &str) -> Vec<String> {
        let mut values = BTreeSet::new();
        for metadata in self.store.catalog.entries.values() {
            let value = match field {
                "type" => Some(metadata.entry_type.as_str()),
                "story" => metadata.story_id.as_deref(),
                "quest" => metadata.quest_id.as_deref(),
                "owner" => metadata.owner_id.as_deref(),
                _ => None,
            };
            if let Some(value) = value.filter(|value| !value.is_empty()) {
                values.insert(value.to_string());
            }
        }
        values.into_iter().collect()
    }

    fn apply_action(&mut self, action: EditorAction) {
        let Some(key) = self.selected.clone() else {
            self.notify("Select an entry first.", true);
            return;
        };
        let result = match action {
            EditorAction::Save => {
                self.store
                    .save_translation(&self.locale, &key, self.draft.clone())
            }
            EditorAction::MarkOnReview => self.store.mark_on_review(&self.locale, &key),
            EditorAction::Accept => self.store.accept(&self.locale, &key),
            EditorAction::Reject => self.store.reject(&self.locale, &key),
            EditorAction::Reset => self.store.reset_translation(&self.locale, &key),
        };
        match result {
            Ok(()) => {
                self.draft_dirty = false;
                self.load_draft();
                self.notify(
                    match action {
                        EditorAction::Save => {
                            "Translation saved and marked waiting for translation."
                        }
                        EditorAction::MarkOnReview => "Translation marked on review.",
                        EditorAction::Accept => "Translation accepted.",
                        EditorAction::Reject => "Translation returned to waiting for translation.",
                        EditorAction::Reset => "Translation reset.",
                    },
                    false,
                );
            }
            Err(error) => self.notify(error.to_string(), true),
        }
    }

    fn apply_context_action(&mut self, key: &str, action: ContextAction) {
        let result = match action {
            ContextAction::MarkWaiting | ContextAction::Reject => {
                self.store.reject(&self.locale, key)
            }
            ContextAction::MarkOnReview => self.store.mark_on_review(&self.locale, key),
            ContextAction::Accept => self.store.accept(&self.locale, key),
            ContextAction::ShowUsages | ContextAction::ShowMetadata => Ok(()),
        };
        match result {
            Ok(()) => {
                if self.selected.as_deref() == Some(key) {
                    self.load_draft();
                }
                self.notify("Entry updated.", false);
            }
            Err(error) => self.notify(error.to_string(), true),
        }
        if matches!(
            action,
            ContextAction::ShowUsages | ContextAction::ShowMetadata
        ) {
            self.selected = Some(key.to_string());
            self.show_inspector = true;
            if action == ContextAction::ShowMetadata {
                self.show_metadata = true;
            }
        }
    }

    fn check_external_changes(&mut self) {
        let mut changed = false;
        while self.external_change.try_recv().is_ok() {
            changed = true;
        }
        if !changed {
            return;
        }
        match Store::load(&self.root) {
            Ok(mut fresh) => match fresh.sync() {
                Ok(_) => {
                    if self.draft_dirty {
                        self.external_snapshot = Some(fresh);
                        self.external_conflict = true;
                    } else {
                        self.store = fresh;
                        self.load_draft();
                        self.notify("Catalog reloaded after an external change.", false);
                    }
                }
                Err(error) => self.notify(format!("External sync failed: {error}"), true),
            },
            Err(error) => self.notify(format!("External reload failed: {error}"), true),
        }
    }

    fn keyboard_action(&mut self, context: &egui::Context, visible_keys: &[String]) {
        let (command, shift, up, down, enter, escape) = context.input(|input| {
            (
                input.modifiers.command,
                input.modifiers.shift,
                input.key_pressed(egui::Key::ArrowUp),
                input.key_pressed(egui::Key::ArrowDown),
                input.key_pressed(egui::Key::Enter),
                input.key_pressed(egui::Key::Escape),
            )
        });
        if command && context.input(|input| input.key_pressed(egui::Key::F)) {
            context.memory_mut(|memory| memory.request_focus(egui::Id::new("global_search")));
        }
        if command && context.input(|input| input.key_pressed(egui::Key::S)) {
            self.apply_action(EditorAction::Save);
        } else if command && enter && shift {
            self.apply_action(EditorAction::Accept);
        } else if command && enter {
            self.apply_action(EditorAction::MarkOnReview);
        } else if escape {
            self.confirm_reset = false;
            self.external_conflict = false;
            self.show_compare = false;
        } else if !context.wants_keyboard_input() && (up || down) && !visible_keys.is_empty() {
            if self.draft_dirty {
                self.notify(
                    "Save or reset the current draft before changing selection.",
                    true,
                );
                return;
            }
            let current = self
                .selected
                .as_ref()
                .and_then(|key| visible_keys.iter().position(|candidate| candidate == key));
            let next = match (current, up, down) {
                (Some(index), true, _) => index.saturating_sub(1),
                (Some(index), _, true) => (index + 1).min(visible_keys.len() - 1),
                (None, _, true) => 0,
                (None, _, false) => visible_keys.len() - 1,
                _ => 0,
            };
            self.request_selection(visible_keys[next].clone());
        } else if enter && !context.wants_keyboard_input() && self.selected.is_none() {
            if let Some(key) = visible_keys.first() {
                self.request_selection(key.clone());
            }
        }
    }

    fn save_content(&mut self) {
        match self.store.content.save(&self.root) {
            Ok(()) => {
                self.content_dirty = false;
                self.notify("Content saved.", false);
            }
            Err(error) => self.notify(format!("Content save failed: {error}"), true),
        }
    }

    fn content_story(&self) -> Option<&content::StoryDefinition> {
        self.selected_story_id
            .as_ref()
            .and_then(|id| self.store.content.stories.get(id))
    }

    fn ensure_content_selection(&mut self) {
        if self.selected_story_id.is_none() {
            self.selected_story_id = self.store.content.stories.keys().next().cloned();
        }
        if let Some(story) = self.content_story() {
            if self
                .selected_node_id
                .as_ref()
                .map(|node| story.nodes.contains_key(node))
                != Some(true)
            {
                self.selected_node_id = Some(story.entry_node_id.clone());
            }
        }
    }

    fn show_content_studio(&mut self, context: &egui::Context) {
        self.ensure_content_selection();
        egui::TopBottomPanel::top("content_studio_toolbar")
            .frame(panel_frame(panel_background(), 8.0))
            .show(context, |ui| {
                toolbar_row(ui, 32.0, |ui| {
                    ui.label(egui::RichText::new("Content authoring").strong().size(16.0));
                    ui.separator();
                    for (section, label) in [
                        (ContentSection::Stories, "Stories"),
                        (ContentSection::Quests, "Quests"),
                        (ContentSection::Dialogues, "Dialogues"),
                        (ContentSection::Items, "Items"),
                        (ContentSection::Entities, "Entities"),
                    ] {
                        if ui
                            .selectable_label(self.content_section == section, label)
                            .clicked()
                        {
                            self.content_section = section;
                            self.selected_node_id = None;
                            self.content_view = ContentView::Graph;
                        }
                    }
                });
                ui.add_space(4.0);
                toolbar_row(ui, 32.0, |ui| {
                    ui.label("View");
                    ui.separator();
                    for (view, label) in [
                        (ContentView::Graph, "Graph"),
                        (ContentView::Sequence, "Sequence"),
                        (ContentView::Reader, "Reader"),
                    ] {
                        if ui
                            .selectable_label(self.content_view == view, label)
                            .clicked()
                        {
                            self.content_view = view;
                            if view == ContentView::Reader {
                                self.start_reader_for_selected_story();
                            }
                        }
                    }
                    ui.separator();
                    if self.content_dirty {
                        ui.colored_label(
                            egui::Color32::from_rgb(236, 173, 72),
                            "● Unsaved changes",
                        );
                    }
                    if ui.button("Save content").clicked() {
                        self.save_content();
                    }
                    if ui.button("Validate").clicked() {
                        match self.store.validate() {
                            Ok(_) => self.notify("Content catalog is valid.", false),
                            Err(error) => self.notify(error.to_string(), true),
                        }
                    }
                });
            });

        if self.content_section != ContentSection::Stories {
            self.show_catalog_section(context);
            return;
        }

        egui::SidePanel::left("story_explorer")
            .default_width(EXPLORER_WIDTH)
            .width_range(SIDE_PANEL_MIN_WIDTH..=SIDE_PANEL_MAX_WIDTH)
            .resizable(true)
            .frame(panel_frame(panel_background(), 12.0))
            .show(context, |ui| {
                constrain_side_panel_content(ui);
                ui.heading("Stories");
                ui.horizontal(|ui| {
                    // Reserve a fixed slot for the action button. Keeping the
                    // sum exact prevents this row from increasing the panel's
                    // natural width by a few pixels on the first frame.
                    let action_width = 84.0;
                    let search_width = (ui.available_width() - action_width - 8.0).max(100.0);
                    ui.add_sized(
                        [search_width, 28.0],
                        egui::TextEdit::singleline(&mut self.content_search)
                            .hint_text("Search stories, nodes, quests..."),
                    );
                    if ui
                        .add_sized([action_width, 28.0], egui::Button::new("New story"))
                        .clicked()
                    {
                        match self.store.create_story() {
                            Ok(created) => {
                                self.selected_story_id = Some(created.story_id.clone());
                                self.selected_node_id = Some(created.entry_node_id.clone());
                                self.content_section = ContentSection::Stories;
                                self.content_view = ContentView::Graph;
                                self.content_dirty = true;
                                self.notify(
                                    format!(
                                        "Created story {} with an initial dialogue.",
                                        created.story_id
                                    ),
                                    false,
                                );
                            }
                            Err(error) => self.notify(format!("New story failed: {error}"), true),
                        }
                    }
                });
                ui.separator();
                egui::ScrollArea::vertical()
                    .id_salt("story_explorer_scroll")
                    .max_width(ui.available_width())
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        let search = self.content_search.to_lowercase();
                        let ids: Vec<String> = self
                            .store
                            .content
                            .stories
                            .iter()
                            .filter(|(id, story)| {
                                search.is_empty()
                                    || id.to_lowercase().contains(&search)
                                    || story.name_key.to_lowercase().contains(&search)
                                    || story
                                        .nodes
                                        .keys()
                                        .any(|node| node.to_lowercase().contains(&search))
                            })
                            .map(|(id, _)| id.clone())
                            .collect();
                        for id in ids {
                            let selected = self.selected_story_id.as_deref() == Some(id.as_str());
                            let node_count = self
                                .store
                                .content
                                .stories
                                .get(&id)
                                .map(|story| story.nodes.len())
                                .unwrap_or_default();
                            if ui
                                .add_sized(
                                    [ui.available_width(), 30.0],
                                    egui::SelectableLabel::new(
                                        selected,
                                        format!("{id}  ·  {node_count} nodes"),
                                    ),
                                )
                                .clicked()
                            {
                                self.selected_story_id = Some(id);
                                self.selected_node_id = None;
                                self.content_view = ContentView::Graph;
                            }
                        }
                    });
            });

        if self.show_inspector {
            self.show_story_inspector(context);
        }
        egui::CentralPanel::default()
            .frame(panel_frame(app_background(), 14.0))
            .show(context, |ui| {
                let Some(story_id) = self.selected_story_id.clone() else {
                    ui.centered_and_justified(|ui| ui.heading("Create or select a story"));
                    return;
                };
                let Some((node_count, entry_node_id, node_ids)) =
                    self.store.content.stories.get(&story_id).map(|story| {
                        (
                            story.nodes.len(),
                            story.entry_node_id.clone(),
                            story.nodes.keys().cloned().collect::<Vec<_>>(),
                        )
                    })
                else {
                    return;
                };
                ui.horizontal(|ui| {
                    ui.heading(&story_id);
                    ui.separator();
                    ui.label(format!("{} nodes · entry: {}", node_count, entry_node_id));
                    if ui.button("Arrange vertically").clicked() {
                        self.arrange_story(&story_id);
                    }
                    if ui.button("Reset layout").clicked() {
                        self.arrange_story(&story_id);
                    }
                    if ui.button("Add node").clicked() {
                        let node_id = next_content_id("node", node_ids.iter());
                        if let Some(story) = self.store.content.stories.get_mut(&story_id) {
                            story.nodes.insert(
                                node_id.clone(),
                                content::StoryNode {
                                    node_type: "dialogue".into(),
                                    payload: None,
                                    name_key: format!("story.{story_id}.node.{node_id}"),
                                    chapter_id: story
                                        .chapters
                                        .first()
                                        .map(|chapter| chapter.id.clone()),
                                    dialogue_id: None,
                                    dialogue_line_id: None,
                                    quest_id: None,
                                    scene_id: None,
                                    event_id: None,
                                    action: None,
                                    condition: None,
                                    choices: Vec::new(),
                                    next: Vec::new(),
                                    position: [120.0, 220.0],
                                },
                            );
                            if let Some(chapter) = story.chapters.first_mut() {
                                chapter.node_ids.push(node_id.clone());
                            }
                        }
                        self.selected_node_id = Some(node_id);
                        self.content_dirty = true;
                    }
                    ui.colored_label(
                        egui::Color32::from_rgb(83, 169, 230),
                        "Preview is simulated",
                    );
                });
                ui.separator();
                match self.content_view {
                    ContentView::Reader => self.show_story_reader(ui, &story_id),
                    ContentView::Sequence => self.show_story_sequence(ui, &story_id),
                    ContentView::Graph => self.show_story_graph(ui, &story_id),
                }
            });
    }

    fn show_story_graph(&mut self, ui: &mut egui::Ui, story_id: &str) {
        let Some(story) = self.store.content.stories.get(story_id) else {
            return;
        };
        let nodes: Vec<(String, content::StoryNode)> = story
            .nodes
            .iter()
            .map(|(id, node)| (id.clone(), node.clone()))
            .collect();
        let (layout, issues) = content::arrange_story_top_down(story);
        let horizontal_legacy_layout = nodes.len() > 1
            && nodes
                .iter()
                .all(|(_, node)| (node.position[1] - nodes[0].1.position[1]).abs() < 0.1);
        let raw_positions: BTreeMap<String, [f32; 2]> = nodes
            .iter()
            .map(|(id, node)| {
                let position = if horizontal_legacy_layout {
                    layout
                        .get(id)
                        .map(|position| [position.x, position.y])
                        .unwrap_or(node.position)
                } else {
                    node.position
                };
                (id.clone(), position)
            })
            .collect();

        // The content layout uses a deterministic coordinate system so saved
        // positions remain reproducible. Normalize that system to the visible
        // graph viewport before painting: a linear story must be centered in
        // the editor, not centered in an arbitrary 1440px canvas.
        let min_x = raw_positions
            .values()
            .map(|position| position[0])
            .fold(f32::INFINITY, f32::min);
        let max_raw_x = raw_positions
            .values()
            .map(|position| position[0] + content::NODE_WIDTH)
            .fold(0.0, f32::max);
        let content_width = (max_raw_x - min_x).max(content::NODE_WIDTH);
        let visible_width = ui.available_width().max(content::NODE_WIDTH + 32.0);
        let canvas_width = visible_width.max(content_width + 96.0);
        let x_offset = (canvas_width - content_width) / 2.0 - min_x;
        let display_positions: BTreeMap<String, [f32; 2]> = raw_positions
            .into_iter()
            .map(|(id, [x, y])| (id, [x + x_offset, y]))
            .collect();

        let mut max_x: f32 = canvas_width;
        let mut max_y: f32 = 560.0;
        for (id, node) in &nodes {
            let position = display_positions.get(id).copied().unwrap_or(node.position);
            max_x = max_x.max(position[0] + content::NODE_WIDTH + 80.0);
            max_y = max_y.max(position[1] + content::NODE_HEIGHT + 120.0);
        }
        egui::ScrollArea::both()
            .id_salt("story_graph_scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let (response, painter) =
                    ui.allocate_painter(egui::vec2(max_x, max_y), egui::Sense::hover());
                let origin = response.rect.min;
                if !issues.orphaned_nodes.is_empty() || !issues.cycle_edges.is_empty() {
                    let mut message = String::from("Graph issues: ");
                    if !issues.orphaned_nodes.is_empty() {
                        message.push_str(&format!("{} orphaned", issues.orphaned_nodes.len()));
                    }
                    if !issues.cycle_edges.is_empty() {
                        if !issues.orphaned_nodes.is_empty() {
                            message.push_str(", ");
                        }
                        message.push_str(&format!("{} cycle edge(s)", issues.cycle_edges.len()));
                    }
                    ui.colored_label(egui::Color32::from_rgb(235, 150, 120), message);
                }
                for (id, node) in &nodes {
                    let position = display_positions.get(id).copied().unwrap_or(node.position);
                    let from = origin
                        + egui::vec2(
                            position[0] + content::NODE_WIDTH / 2.0,
                            position[1] + content::NODE_HEIGHT,
                        );
                    for next in &node.next {
                        if let Some((_, target)) =
                            nodes.iter().find(|(candidate, _)| candidate == next)
                        {
                            let target_position = display_positions
                                .get(next)
                                .copied()
                                .unwrap_or(target.position);
                            let to = origin
                                + egui::vec2(
                                    target_position[0] + content::NODE_WIDTH / 2.0,
                                    target_position[1],
                                );
                            painter.line_segment(
                                [from, to],
                                egui::Stroke::new(2.0, egui::Color32::from_rgb(92, 130, 165)),
                            );
                        }
                    }
                    for choice in &node.choices {
                        if let Some((_, target)) = nodes
                            .iter()
                            .find(|(candidate, _)| candidate == &choice.next)
                        {
                            let target_position = display_positions
                                .get(&choice.next)
                                .copied()
                                .unwrap_or(target.position);
                            let to = origin
                                + egui::vec2(
                                    target_position[0] + content::NODE_WIDTH / 2.0,
                                    target_position[1],
                                );
                            painter.line_segment(
                                [from, to],
                                egui::Stroke::new(2.0, egui::Color32::from_rgb(236, 173, 72)),
                            );
                        }
                    }
                    let rect = egui::Rect::from_min_size(
                        origin + egui::vec2(position[0], position[1]),
                        egui::vec2(content::NODE_WIDTH, content::NODE_HEIGHT),
                    );
                    let selected = self.selected_node_id.as_deref() == Some(id.as_str());
                    let [r, g, b] = content::node_type_color(&node.node_type);
                    let semantic_fill = egui::Color32::from_rgb(r, g, b);
                    let fill = if selected {
                        egui::Color32::from_rgb(45, 96, 145)
                    } else {
                        semantic_fill
                    };
                    let invalid = issues.orphaned_nodes.contains(id)
                        || issues
                            .cycle_edges
                            .iter()
                            .any(|(from, to)| from == id || to == id);
                    let (node_text, speaker, used_fallback) = self.story_node_text(node);
                    painter.rect_filled(rect, 8.0, fill);
                    painter.rect_stroke(
                        rect,
                        8.0,
                        egui::Stroke::new(
                            if invalid { 3.0 } else { 1.0 },
                            if invalid {
                                egui::Color32::from_rgb(235, 150, 120)
                            } else if selected {
                                egui::Color32::WHITE
                            } else {
                                egui::Color32::from_rgb(84, 104, 125)
                            },
                        ),
                    );
                    painter.text(
                        rect.left_top() + egui::vec2(10.0, 10.0),
                        egui::Align2::LEFT_TOP,
                        truncate_for_node(&node_text, 30),
                        egui::FontId::proportional(14.0),
                        egui::Color32::WHITE,
                    );
                    painter.text(
                        rect.left_top() + egui::vec2(10.0, 32.0),
                        egui::Align2::LEFT_TOP,
                        truncate_for_node(&format!("{} · {}", speaker, node.node_type), 30),
                        egui::FontId::proportional(12.0),
                        egui::Color32::from_rgb(174, 191, 210),
                    );
                    painter.text(
                        rect.left_top() + egui::vec2(10.0, 54.0),
                        egui::Align2::LEFT_TOP,
                        truncate_for_node(
                            if used_fallback {
                                "EN fallback"
                            } else {
                                node.dialogue_id
                                    .as_deref()
                                    .or(node.quest_id.as_deref())
                                    .unwrap_or("No content")
                            },
                            30,
                        ),
                        egui::FontId::proportional(12.0),
                        if used_fallback {
                            egui::Color32::from_rgb(236, 173, 72)
                        } else {
                            egui::Color32::from_rgb(174, 191, 210)
                        },
                    );
                    let response = ui.interact(
                        rect,
                        egui::Id::new(("story_node", story_id, id)),
                        egui::Sense::click_and_drag(),
                    );
                    if response.clicked() {
                        self.selected_node_id = Some(id.clone());
                    }
                    if response.dragged() {
                        if let Some(story) = self.store.content.stories.get_mut(story_id) {
                            if let Some(node) = story.nodes.get_mut(id) {
                                let delta = response.drag_delta();
                                node.position[0] += delta.x;
                                node.position[1] += delta.y;
                                self.content_dirty = true;
                            }
                        }
                    }
                }
            });
    }

    fn arrange_story(&mut self, story_id: &str) {
        let Some(story) = self.store.content.stories.get(story_id) else {
            return;
        };
        let (positions, issues) = content::arrange_story_top_down(story);
        if let Some(story) = self.store.content.stories.get_mut(story_id) {
            for (node_id, position) in positions {
                if let Some(node) = story.nodes.get_mut(&node_id) {
                    node.position = [position.x, position.y];
                }
            }
        }
        self.content_dirty = true;
        if issues.orphaned_nodes.is_empty() && issues.cycle_edges.is_empty() {
            self.notify("Story arranged top-down.", false);
        } else {
            self.notify(
                format!(
                    "Story arranged with {} orphaned node(s) and {} cycle edge(s) highlighted.",
                    issues.orphaned_nodes.len(),
                    issues.cycle_edges.len()
                ),
                true,
            );
        }
    }

    fn show_story_sequence(&mut self, ui: &mut egui::Ui, story_id: &str) {
        let Some(story) = self.store.content.stories.get(story_id) else {
            return;
        };
        let mut node_ids = Vec::new();
        let mut current = story.entry_node_id.clone();
        let mut seen = BTreeSet::new();
        while seen.insert(current.clone()) {
            node_ids.push(current.clone());
            let Some(node) = story.nodes.get(&current) else {
                break;
            };
            let Some(next) = node.next.first() else {
                break;
            };
            current = next.clone();
        }
        for (index, node_id) in node_ids.iter().enumerate() {
            let Some(node) = story.nodes.get(node_id) else {
                continue;
            };
            if ui
                .selectable_label(
                    self.selected_node_id.as_deref() == Some(node_id),
                    format!("{:02}  {}  ·  {}", index + 1, node_id, node.node_type),
                )
                .clicked()
            {
                self.selected_node_id = Some(node_id.clone());
            }
            ui.indent(node_id, |ui| {
                ui.label(
                    node.dialogue_id
                        .as_deref()
                        .or(node.quest_id.as_deref())
                        .unwrap_or("No linked content"),
                );
                if let Some(dialogue_id) = &node.dialogue_id {
                    if let Some(dialogue) = self.store.content.dialogues.get(dialogue_id) {
                        for line in &dialogue.lines {
                            if line.kind == "text" {
                                if let Some(key) = &line.key {
                                    let preview = self.localized_preview(key);
                                    ui.label(format!(
                                        "{}: {}",
                                        line.speaker.as_deref().unwrap_or("Narrator"),
                                        preview.text
                                    ));
                                }
                            } else if let Some(command) = &line.command {
                                ui.colored_label(
                                    egui::Color32::from_rgb(236, 173, 72),
                                    format!("Command: {command}"),
                                );
                            }
                        }
                    }
                }
                ui.label(format!(
                    "Next: {}",
                    if node.next.is_empty() {
                        "end".into()
                    } else {
                        node.next.join(", ")
                    }
                ));
            });
        }
    }

    fn start_reader_for_selected_story(&mut self) {
        let Some(story_id) = self.selected_story_id.clone() else {
            return;
        };
        self.reader.locale = self.locale.clone();
        if self.reader.story_id.as_deref() != Some(story_id.as_str()) {
            self.reader.story_id = Some(story_id.clone());
            self.reader.current_node_id = self
                .store
                .content
                .stories
                .get(&story_id)
                .map(|story| story.entry_node_id.clone());
            self.reader.history.clear();
            self.reader.selected_choice = None;
            self.reader.active = true;
        } else if self.reader.current_node_id.is_none() {
            self.reader.current_node_id = self
                .store
                .content
                .stories
                .get(&story_id)
                .map(|story| story.entry_node_id.clone());
            self.reader.active = true;
        }
    }

    fn reader_restart(&mut self) {
        let Some(story_id) = self.reader.story_id.clone() else {
            return;
        };
        let entry_node_id = self
            .store
            .content
            .stories
            .get(&story_id)
            .map(|story| story.entry_node_id.clone());
        if let Some(entry_node_id) = entry_node_id {
            self.reader
                .restart_at(story_id, entry_node_id, self.locale.clone());
        }
    }

    fn reader_back(&mut self) {
        self.reader.back();
    }

    fn reader_advance_to(&mut self, next: String) {
        self.reader.advance_to(next);
    }

    pub(crate) fn localized_preview(&self, key: &str) -> LocalizedPreview {
        let source = self.store.source.entries.get(key);
        let source_text = source
            .map(|entry| entry.text.clone())
            .unwrap_or_else(|| format!("Missing key: {key}"));
        if self.locale == self.store.catalog.source_locale {
            return LocalizedPreview {
                text: source_text,
                used_fallback: false,
                status: "source".into(),
            };
        }
        let current_source_hash = source.map(|entry| hash_text(&entry.text));
        if let Some(entry) = self
            .store
            .targets
            .get(&self.locale)
            .and_then(|file| file.entries.get(key))
        {
            if entry.status == ACCEPTED && Some(entry.source_hash.clone()) == current_source_hash {
                return LocalizedPreview {
                    text: entry.text.clone(),
                    used_fallback: false,
                    status: ACCEPTED.into(),
                };
            }
            return LocalizedPreview {
                text: source_text,
                used_fallback: true,
                status: entry.status.clone(),
            };
        }
        LocalizedPreview {
            text: source_text,
            used_fallback: true,
            status: "missing".into(),
        }
    }

    fn story_node_text(&self, node: &content::StoryNode) -> (String, String, bool) {
        if let Some(dialogue_id) = &node.dialogue_id {
            if let Some(dialogue) = self.store.content.dialogues.get(dialogue_id) {
                for line in &dialogue.lines {
                    if line.kind != "text" {
                        continue;
                    }
                    if let Some(key) = &line.key {
                        let preview = self.localized_preview(key);
                        return (
                            preview
                                .text
                                .replace('\n', " ")
                                .split_whitespace()
                                .collect::<Vec<_>>()
                                .join(" "),
                            line.speaker.clone().unwrap_or_else(|| "Narrator".into()),
                            preview.used_fallback,
                        );
                    }
                }
                if !dialogue.name_key.is_empty() {
                    let preview = self.localized_preview(&dialogue.name_key);
                    return (preview.text, "Dialogue".into(), preview.used_fallback);
                }
            }
            return (dialogue_id.clone(), "Dialogue".into(), false);
        }
        if !node.name_key.is_empty() {
            let preview = self.localized_preview(&node.name_key);
            return (preview.text, node.node_type.clone(), preview.used_fallback);
        }
        if let Some(quest_id) = &node.quest_id {
            if let Some(quest) = self.store.content.quests.get(quest_id) {
                if !quest.name_key.is_empty() {
                    let preview = self.localized_preview(&quest.name_key);
                    return (preview.text, "Quest".into(), preview.used_fallback);
                }
            }
            return (quest_id.clone(), "Quest".into(), false);
        }
        if let Some(action) = &node.action {
            return (action.type_name().into(), "Action".into(), false);
        }
        if let Some(condition) = &node.condition {
            return (condition.condition_type.clone(), "Condition".into(), false);
        }
        (node.node_type.clone(), "Node".into(), false)
    }

    fn show_story_reader(&mut self, ui: &mut egui::Ui, story_id: &str) {
        self.start_reader_for_selected_story();
        self.reader.locale = self.locale.clone();
        let Some(current_node_id) = self.reader.current_node_id.clone() else {
            ui.centered_and_justified(|ui| ui.heading("Select a story to start reading"));
            return;
        };
        let Some(story) = self.store.content.stories.get(story_id) else {
            return;
        };
        let Some(node) = story.nodes.get(&current_node_id).cloned() else {
            ui.colored_label(
                egui::Color32::from_rgb(235, 150, 120),
                "Reader stopped: current node no longer exists.",
            );
            return;
        };
        let chapter = node.chapter_id.as_deref().unwrap_or("main");
        let story_name = if story.name_key.is_empty() {
            story_id.to_string()
        } else {
            self.localized_preview(&story.name_key).text
        };
        ui.horizontal(|ui| {
            ui.heading(format!("{story_name} · Story reader"));
            ui.separator();
            ui.label(format!("Chapter: {chapter}"));
            ui.label(format!("Node: {current_node_id}"));
            if self.reader.history.is_empty() {
                ui.colored_label(egui::Color32::from_rgb(136, 215, 164), "At start");
            }
        });
        ui.label("Preview only. Actions and callbacks are simulated and never mutate a game save.");
        ui.horizontal(|ui| {
            if ui
                .add_enabled(!self.reader.history.is_empty(), egui::Button::new("Back"))
                .clicked()
            {
                self.reader_back();
            }
            if ui.button("Restart").clicked() {
                self.reader_restart();
            }
            ui.label(format!("Locale: {}", self.locale));
        });

        let mut transition: Option<String> = None;
        egui::ScrollArea::vertical()
            .id_salt("story_reader_scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.group(|ui| {
                    ui.label(format!("{} node", node.node_type));
                    if let Some(dialogue_id) = &node.dialogue_id {
                        if let Some(dialogue) = self.store.content.dialogues.get(dialogue_id) {
                            for line in &dialogue.lines {
                                if line.kind == "text" {
                                    if let Some(key) = &line.key {
                                        let preview = self.localized_preview(key);
                                        ui.horizontal_wrapped(|ui| {
                                            ui.strong(format!(
                                                "{}:",
                                                line.speaker.as_deref().unwrap_or("Narrator")
                                            ));
                                            ui.label(&preview.text);
                                        });
                                        if preview.used_fallback {
                                            ui.colored_label(
                                                egui::Color32::from_rgb(236, 173, 72),
                                                format!(
                                                    "Fallback to English ({})",
                                                    status_display(&preview.status)
                                                ),
                                            );
                                        }
                                    }
                                } else if let Some(command) = &line.command {
                                    ui.colored_label(
                                        egui::Color32::from_rgb(236, 173, 72),
                                        format!("Non-translatable command · {command}"),
                                    );
                                }
                            }
                        } else {
                            ui.colored_label(
                                egui::Color32::from_rgb(235, 150, 120),
                                "Missing dialogue definition",
                            );
                        }
                    }
                    if let Some(condition) = &node.condition {
                        ui.colored_label(
                            egui::Color32::from_rgb(196, 158, 46),
                            format!("Condition simulated · {}", condition.condition_type),
                        );
                    }
                    if let Some(action) = &node.action {
                        ui.colored_label(
                            egui::Color32::from_rgb(208, 116, 45),
                            format!("Action simulated · {}", action.type_name()),
                        );
                        if matches!(action, content::ActionDefinition::Callback { .. }) {
                            ui.colored_label(
                                egui::Color32::from_rgb(235, 150, 120),
                                "Java callback required",
                            );
                        }
                    }
                    if let Some(quest_id) = &node.quest_id {
                        ui.label(format!("Quest affected · {quest_id}"));
                    }
                    if node.node_type == "end" {
                        ui.colored_label(egui::Color32::from_rgb(136, 215, 164), "End of story");
                    }
                });

                if !node.choices.is_empty() {
                    ui.separator();
                    ui.strong("Choose a branch");
                    for choice in &node.choices {
                        let label = if choice.label_key.is_empty() {
                            choice.id.clone()
                        } else {
                            self.localized_preview(&choice.label_key).text
                        };
                        if ui.button(label).clicked() {
                            transition = Some(choice.next.clone());
                            self.reader.selected_choice = Some(choice.id.clone());
                        }
                    }
                } else if let Some(next) = node.next.first() {
                    if ui.button("Next").clicked() {
                        transition = Some(next.clone());
                    }
                } else if node.node_type != "end" {
                    ui.colored_label(
                        egui::Color32::from_rgb(235, 150, 120),
                        "No outgoing connection",
                    );
                }
            });
        if let Some(next) = transition {
            self.reader_advance_to(next);
        }
    }

    fn show_story_inspector(&mut self, context: &egui::Context) {
        let story_id = self.selected_story_id.clone();
        let node_id = self.selected_node_id.clone();
        let mut delete_node = false;
        egui::SidePanel::right("story_inspector")
            .default_width(INSPECTOR_WIDTH)
            .width_range(SIDE_PANEL_MIN_WIDTH..=SIDE_PANEL_MAX_WIDTH)
            .resizable(true)
            .frame(panel_frame(panel_background(), 12.0))
            .show(context, |ui| {
                constrain_side_panel_content(ui);
                ui.heading("Inspector");
                egui::ScrollArea::vertical()
                    .max_width(ui.available_width())
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                    let Some(story_id) = story_id.as_deref() else { ui.label("Select a story."); return; };
                    if let Some(node_id) = node_id.as_deref() {
                        ui.label(format!("Node ID: {node_id}"));
                        if let Some(story) = self.store.content.stories.get_mut(story_id) {
                            let options: Vec<String> = story
                                .nodes
                                .keys()
                                .filter(|candidate| *candidate != node_id)
                                .cloned()
                                .collect();
                            if let Some(node) = story.nodes.get_mut(node_id) {
                                ui.horizontal(|ui| { ui.label("Type"); egui::ComboBox::from_id_salt("node_type").selected_text(&node.node_type).show_ui(ui, |ui| { for value in ["dialogue","choice","condition","action","quest_start","quest_update","scene_transition","event","end"] { ui.selectable_value(&mut node.node_type, value.into(), value); } }); });
                                ui.label("Display name key");
                                if ui.text_edit_singleline(&mut node.name_key).changed() { self.content_dirty = true; }
                                if let Some(value) = &mut node.dialogue_id {
                                    ui.label("Dialogue ID"); if ui.text_edit_singleline(value).changed() { self.content_dirty = true; }
                                } else if ui.button("Link dialogue").clicked() { node.dialogue_id = Some(String::new()); self.content_dirty = true; }
                                if let Some(value) = &mut node.quest_id {
                                    ui.label("Quest ID"); if ui.text_edit_singleline(value).changed() { self.content_dirty = true; }
                                } else if ui.button("Link quest").clicked() { node.quest_id = Some(String::new()); self.content_dirty = true; }
                                ui.separator();
                                ui.label("Next nodes");
                                for next in &node.next { ui.label(format!("→ {next}")); }
                                egui::ComboBox::from_id_salt("add_next_node").selected_text("Add connection...").show_ui(ui, |ui| { for target in options { if ui.button(&target).clicked() { if !node.next.contains(&target) { node.next.push(target); self.content_dirty = true; } ui.close_menu(); } } });
                                if !node.choices.is_empty() { ui.label(format!("{} choice outputs", node.choices.len())); }
                                if node.action.is_some() { ui.label("Has declarative action"); }
                                if node.condition.is_some() { ui.label("Has condition"); }
                                if node_id != story.entry_node_id && ui.button("Delete node").clicked() { delete_node = true; }
                            }
                        }
                    } else if let Some(story) = self.store.content.stories.get_mut(story_id) {
                        ui.label(format!("Story ID: {story_id}"));
                        ui.label("Name key"); if ui.text_edit_singleline(&mut story.name_key).changed() { self.content_dirty = true; }
                        ui.label("Description key"); if ui.text_edit_singleline(&mut story.description_key).changed() { self.content_dirty = true; }
                        ui.label("Owner ID"); if ui.text_edit_singleline(&mut story.owner_id).changed() { self.content_dirty = true; }
                        ui.label(format!("Entry node: {}", story.entry_node_id));
                        ui.label(format!("{} chapters", story.chapters.len()));
                        ui.colored_label(egui::Color32::from_rgb(174, 191, 210), "IDs are stable technical identifiers. Edit name keys instead of renaming IDs.");
                    }
                });
            });
        if delete_node {
            if let (Some(story_id), Some(node_id)) = (story_id, node_id) {
                if let Some(story) = self.store.content.stories.get_mut(&story_id) {
                    story.nodes.remove(&node_id);
                    for other in story.nodes.values_mut() {
                        other.next.retain(|next| next != &node_id);
                        other.choices.retain(|choice| choice.next != node_id);
                    }
                    for chapter in &mut story.chapters {
                        chapter.node_ids.retain(|id| id != &node_id);
                    }
                }
                self.selected_node_id = None;
                self.content_dirty = true;
            }
        }
    }

    fn show_catalog_section(&mut self, context: &egui::Context) {
        let section = self.content_section;
        egui::SidePanel::left("catalog_explorer")
            .default_width(EXPLORER_WIDTH)
            .width_range(SIDE_PANEL_MIN_WIDTH..=SIDE_PANEL_MAX_WIDTH)
            .resizable(true)
            .frame(panel_frame(panel_background(), 12.0))
            .show(context, |ui| {
                constrain_side_panel_content(ui);
                ui.heading(match section {
                    ContentSection::Quests => "Quests",
                    ContentSection::Dialogues => "Dialogues",
                    ContentSection::Items => "Items",
                    ContentSection::Entities => "Entities",
                    ContentSection::Stories => "Stories",
                });
                ui.label("Select an entry to edit its stable ID and references.");
                let new_label = match section {
                    ContentSection::Quests => "New quest",
                    ContentSection::Dialogues => "New dialogue",
                    ContentSection::Items => "New item",
                    ContentSection::Entities => "New entity",
                    ContentSection::Stories => "New story",
                };
                if ui.button(new_label).clicked() {
                    let id = match section {
                        ContentSection::Quests => {
                            next_content_id("quest", self.store.content.quests.keys())
                        }
                        ContentSection::Dialogues => {
                            next_content_id("dialogue", self.store.content.dialogues.keys())
                        }
                        ContentSection::Items => {
                            next_content_id("item", self.store.content.items.keys())
                        }
                        ContentSection::Entities => {
                            next_content_id("entity", self.store.content.entities.keys())
                        }
                        ContentSection::Stories => return,
                    };
                    match section {
                        ContentSection::Quests => {
                            self.store.content.quests.insert(
                                id.clone(),
                                content::QuestDefinition {
                                    name_key: String::new(),
                                    description_keys: Vec::new(),
                                    story_id: None,
                                    chapter_id: None,
                                    owner_id: None,
                                    stages: Vec::new(),
                                    requirements: Vec::new(),
                                    start_actions: Vec::new(),
                                    complete_actions: Vec::new(),
                                    fail_actions: Vec::new(),
                                    initial_status: "PENDING".into(),
                                },
                            );
                        }
                        ContentSection::Dialogues => {
                            self.store.content.dialogues.insert(
                                id.clone(),
                                content::DialogueDefinition {
                                    name_key: String::new(),
                                    story_id: None,
                                    chapter_id: None,
                                    owner_id: None,
                                    lines: Vec::new(),
                                    on_enter: Vec::new(),
                                    on_exit: Vec::new(),
                                },
                            );
                        }
                        ContentSection::Items => {
                            self.store.content.items.insert(
                                id.clone(),
                                content::ItemDefinition {
                                    name_key: String::new(),
                                    description_key: String::new(),
                                    category: "misc".into(),
                                    sprite_ref: String::new(),
                                    stackable: true,
                                    max_stack: Some(99),
                                    effects: Vec::new(),
                                    tags: Vec::new(),
                                    quest_item: false,
                                },
                            );
                        }
                        ContentSection::Entities => {
                            self.store.content.entities.insert(
                                id.clone(),
                                content::EntityDefinition {
                                    name_key: String::new(),
                                    entity_type: "npc".into(),
                                    sprite_ref: String::new(),
                                    owner_id: String::new(),
                                    dialogue_ids: Vec::new(),
                                    interaction_conditions: Vec::new(),
                                    interaction_actions: Vec::new(),
                                    states: vec!["default".into()],
                                    initial_state: "default".into(),
                                },
                            );
                        }
                        ContentSection::Stories => {}
                    }
                    self.selected_content_id = Some(id);
                    self.content_dirty = true;
                    self.notify(format!("Created {}.", new_label), false);
                }
                ui.separator();
                egui::ScrollArea::vertical()
                    .max_width(ui.available_width())
                    .show(ui, |ui| match section {
                        ContentSection::Quests => {
                            for id in self.store.content.quests.keys() {
                                if ui
                                    .add_sized(
                                        [ui.available_width(), 30.0],
                                        egui::SelectableLabel::new(
                                            self.selected_content_id.as_deref()
                                                == Some(id.as_str()),
                                            id,
                                        ),
                                    )
                                    .clicked()
                                {
                                    self.selected_content_id = Some(id.clone());
                                }
                            }
                        }
                        ContentSection::Dialogues => {
                            for id in self.store.content.dialogues.keys() {
                                if ui
                                    .add_sized(
                                        [ui.available_width(), 30.0],
                                        egui::SelectableLabel::new(
                                            self.selected_content_id.as_deref()
                                                == Some(id.as_str()),
                                            id,
                                        ),
                                    )
                                    .clicked()
                                {
                                    self.selected_content_id = Some(id.clone());
                                }
                            }
                        }
                        ContentSection::Items => {
                            for id in self.store.content.items.keys() {
                                if ui
                                    .add_sized(
                                        [ui.available_width(), 30.0],
                                        egui::SelectableLabel::new(
                                            self.selected_content_id.as_deref()
                                                == Some(id.as_str()),
                                            id,
                                        ),
                                    )
                                    .clicked()
                                {
                                    self.selected_content_id = Some(id.clone());
                                }
                            }
                        }
                        ContentSection::Entities => {
                            for id in self.store.content.entities.keys() {
                                if ui
                                    .add_sized(
                                        [ui.available_width(), 30.0],
                                        egui::SelectableLabel::new(
                                            self.selected_content_id.as_deref()
                                                == Some(id.as_str()),
                                            id,
                                        ),
                                    )
                                    .clicked()
                                {
                                    self.selected_content_id = Some(id.clone());
                                }
                            }
                        }
                        ContentSection::Stories => {}
                    });
            });
        egui::CentralPanel::default().frame(panel_frame(app_background(), 16.0)).show(context, |ui| {
            let Some(id) = self.selected_content_id.clone() else {
                ui.centered_and_justified(|ui| ui.heading("Create or select an entry"));
                return;
            };
            ui.heading(format!("{} · {}", id, match section { ContentSection::Quests => "Quest", ContentSection::Dialogues => "Dialogue", ContentSection::Items => "Item", ContentSection::Entities => "Entity", ContentSection::Stories => "Story" }));
            ui.label("Stable IDs are read-only. Names, references and gameplay metadata are editable.");
            egui::ScrollArea::vertical().show(ui, |ui| match section {
                ContentSection::Quests => if let Some(value) = self.store.content.quests.get_mut(&id) {
                    edit_text(ui, "Name key", &mut value.name_key, &mut self.content_dirty);
                    edit_text(ui, "Owner ID", value.owner_id.get_or_insert_with(String::new), &mut self.content_dirty);
                    edit_text(ui, "Story ID", value.story_id.get_or_insert_with(String::new), &mut self.content_dirty);
                    edit_text(ui, "Initial status", &mut value.initial_status, &mut self.content_dirty);
                    ui.separator(); ui.strong("Requirements");
                    for requirement in &value.requirements { ui.label(format!("{} · {}", requirement.requirement_type, requirement.item_id.as_deref().or(requirement.entity_id.as_deref()).or(requirement.callback_id.as_deref()).unwrap_or("custom"))); }
                },
                ContentSection::Dialogues => if let Some(value) = self.store.content.dialogues.get_mut(&id) {
                    edit_text(ui, "Name key", &mut value.name_key, &mut self.content_dirty);
                    edit_text(ui, "Owner ID", value.owner_id.get_or_insert_with(String::new), &mut self.content_dirty);
                    edit_text(ui, "Story ID", value.story_id.get_or_insert_with(String::new), &mut self.content_dirty);
                    ui.separator(); ui.strong(format!("{} lines", value.lines.len()));
                    for line in &value.lines { ui.label(format!("{} · {} · {}", line.id, line.speaker.as_deref().unwrap_or("Narrator"), line.key.as_deref().or(line.command.as_deref()).unwrap_or(""))); }
                },
                ContentSection::Items => if let Some(value) = self.store.content.items.get_mut(&id) {
                    edit_text(ui, "Name key", &mut value.name_key, &mut self.content_dirty); edit_text(ui, "Description key", &mut value.description_key, &mut self.content_dirty); edit_text(ui, "Category", &mut value.category, &mut self.content_dirty); edit_text(ui, "Existing sprite ref", &mut value.sprite_ref, &mut self.content_dirty); ui.checkbox(&mut value.stackable, "Stackable"); ui.checkbox(&mut value.quest_item, "Quest item");
                },
                ContentSection::Entities => if let Some(value) = self.store.content.entities.get_mut(&id) {
                    edit_text(ui, "Name key", &mut value.name_key, &mut self.content_dirty); edit_text(ui, "Entity type", &mut value.entity_type, &mut self.content_dirty); edit_text(ui, "Registered sprite ref", &mut value.sprite_ref, &mut self.content_dirty); edit_text(ui, "Owner ID", &mut value.owner_id, &mut self.content_dirty); edit_text(ui, "Initial state", &mut value.initial_state, &mut self.content_dirty); ui.separator(); ui.label(format!("Dialogues: {}", value.dialogue_ids.join(", "))); ui.label(format!("States: {}", value.states.join(", ")));
                },
                ContentSection::Stories => {}
            });
        });
    }

    fn render_notification(&mut self, context: &egui::Context) {
        if let Some(notification) = &self.notification {
            if notification.expires_at > Instant::now() {
                egui::TopBottomPanel::bottom("studio_notification")
                    .frame(panel_frame(toolbar_background(), 8.0))
                    .show(context, |ui| {
                        let color = if notification.is_error {
                            egui::Color32::from_rgb(235, 150, 120)
                        } else {
                            egui::Color32::from_rgb(133, 210, 157)
                        };
                        ui.colored_label(color, &notification.message);
                    });
            } else {
                self.notification = None;
            }
        }
    }
}

impl eframe::App for EditorApp {
    fn update(&mut self, context: &egui::Context, _frame: &mut eframe::Frame) {
        configure_theme(context);
        self.check_external_changes();

        let mut pending_action = None;
        let mut pending_context_action = None;
        let mut locale_changed = false;
        let mut clear_filters = false;
        let mut chip_to_clear = None;
        let previous_locale = self.locale.clone();

        let types = self.values_for("type");
        let stories = self.values_for("story");
        let quests = self.values_for("quest");
        let owners = self.values_for("owner");
        egui::TopBottomPanel::top("app_toolbar")
            .frame(panel_frame(toolbar_background(), 12.0))
            .show(context, |ui| {
                // Keep navigation and actions on deterministic rows. Wrapping a
                // toolbar mid-group makes controls jump to different baselines
                // on small windows and makes the primary actions hard to scan.
                toolbar_row(ui, 34.0, |ui| {
                    ui.add_sized(
                        [178.0, 34.0],
                        egui::Label::new(egui::RichText::new("KStory").size(20.0).strong()),
                    );
                    ui.separator();
                    if ui
                        .add_sized(
                            [144.0, 32.0],
                            egui::SelectableLabel::new(self.show_content, "Content explorer"),
                        )
                        .clicked()
                    {
                        self.show_content = true;
                        self.show_studio = false;
                    }
                    if ui
                        .add_sized(
                            [132.0, 32.0],
                            egui::SelectableLabel::new(self.show_studio, "Content studio"),
                        )
                        .clicked()
                    {
                        self.show_studio = true;
                        self.show_content = false;
                    }
                    if ui
                        .add_sized(
                            [124.0, 32.0],
                            egui::SelectableLabel::new(
                                !self.show_content && !self.show_studio,
                                "Key registry",
                            ),
                        )
                        .clicked()
                    {
                        self.show_content = false;
                        self.show_studio = false;
                    }
                });
                ui.add_space(4.0);
                toolbar_row(ui, 32.0, |ui| {
                    ui.add_sized(
                        [48.0, 30.0],
                        egui::Label::new(egui::RichText::new("Locale").strong()),
                    );
                    egui::ComboBox::from_id_salt("locale_selector")
                        .selected_text(locale_name(&self.locale))
                        .width(108.0)
                        .show_ui(ui, |ui| {
                            for locale in std::iter::once(self.store.catalog.source_locale.clone())
                                .chain(self.store.catalog.target_locales.iter().cloned())
                            {
                                if ui
                                    .selectable_value(
                                        &mut self.locale,
                                        locale.clone(),
                                        locale_name(&locale),
                                    )
                                    .changed()
                                {
                                    locale_changed = true;
                                    ui.memory_mut(|memory| memory.close_popup());
                                }
                            }
                        });
                    if ui
                        .add_sized([70.0, 32.0], egui::Button::new("Sync"))
                        .clicked()
                    {
                        match self.store.sync() {
                            Ok(report) => self.notify(
                                format!(
                                    "Synced: {} new keys, {} source changes, {} orphaned keys.",
                                    report.created_keys,
                                    report.changed_sources,
                                    report.orphaned_keys
                                ),
                                false,
                            ),
                            Err(error) => self.notify(error.to_string(), true),
                        }
                    }
                    if ui
                        .add_sized([76.0, 32.0], egui::Button::new("Export"))
                        .clicked()
                    {
                        match self.store.export() {
                            Ok(()) => self.notify("Runtime bundles exported.", false),
                            Err(error) => self.notify(error.to_string(), true),
                        }
                    }
                    let inspector_label = if self.show_inspector {
                        "Hide inspector"
                    } else {
                        "Show inspector"
                    };
                    if ui
                        .add_sized([116.0, 32.0], egui::Button::new(inspector_label))
                        .clicked()
                    {
                        self.show_inspector = !self.show_inspector;
                    }
                });
                ui.separator();
                toolbar_row(ui, 24.0, |ui| {
                    ui.label(egui::RichText::new("Translation status").strong());
                    let counts = self.store.status_counts(&self.locale);
                    if self.locale == self.store.catalog.source_locale {
                        ui.colored_label(
                            status_color("source"),
                            format!("source: {}", counts.get("source").copied().unwrap_or(0)),
                        );
                    } else {
                        for status in [WAITING, ON_REVIEW, ACCEPTED] {
                            ui.colored_label(
                                status_color(status),
                                format!(
                                    "{}: {}",
                                    status.replace('_', " "),
                                    counts.get(status).copied().unwrap_or(0)
                                ),
                            );
                        }
                    }
                });
            });

        if self.show_studio {
            self.show_content_studio(context);
            self.render_notification(context);
            context.request_repaint_after(Duration::from_millis(250));
            return;
        }

        egui::TopBottomPanel::top("filter_toolbar")
            .frame(panel_frame(panel_background(), 10.0))
            .show(context, |ui| {
                // Keep every row inside an explicit width. The compact variant
                // uses separate control rows so no label or combo can be
                // pushed outside the left or right edge of the window.
                let compact_filters = ui.available_width() < 1000.0;
                let status_options: Vec<&str> = if self.locale == self.store.catalog.source_locale {
                    vec!["source"]
                } else {
                    vec![WAITING, ON_REVIEW, ACCEPTED]
                };

                if compact_filters {
                    toolbar_row(ui, 30.0, |ui| {
                        ui.add_sized(
                            [54.0, 30.0],
                            egui::Label::new(egui::RichText::new("Search").strong()),
                        );
                        let search_width = (ui.available_width() - 2.0).max(150.0);
                        ui.add_sized(
                            [search_width, 30.0],
                            egui::TextEdit::singleline(&mut self.filters.search)
                                .id_salt("global_search")
                                .hint_text(
                                    "key, source, translation, story, quest, owner or speaker",
                                ),
                        );
                    });
                    ui.add_space(6.0);
                    toolbar_row(ui, 30.0, |ui| {
                        let control_width = ((ui.available_width() - 16.0) / 3.0).max(120.0);
                        filter_combo_sized(
                            ui,
                            "Status",
                            &mut self.filters.status,
                            status_options,
                            control_width,
                        );
                        filter_combo_sized(
                            ui,
                            "Type",
                            &mut self.filters.entry_type,
                            types.iter().map(String::as_str),
                            control_width,
                        );
                        sort_combo(ui, &mut self.filters.sort_mode, control_width);
                    });
                    ui.add_space(6.0);
                    toolbar_row(ui, 30.0, |ui| {
                        let control_width = ((ui.available_width() - 16.0) / 3.0).max(120.0);
                        filter_combo_sized(
                            ui,
                            "Story",
                            &mut self.filters.story,
                            stories.iter().map(String::as_str),
                            control_width,
                        );
                        filter_combo_sized(
                            ui,
                            "Quest",
                            &mut self.filters.quest,
                            quests.iter().map(String::as_str),
                            control_width,
                        );
                        filter_combo_sized(
                            ui,
                            "Owner",
                            &mut self.filters.owner,
                            owners.iter().map(String::as_str),
                            control_width,
                        );
                    });
                    ui.add_space(6.0);
                    toolbar_row(ui, 30.0, |ui| {
                        ui.checkbox(&mut self.filters.only_orphaned, "Orphaned only");
                        if ui
                            .add_sized([112.0, 30.0], egui::Button::new("Clear filters"))
                            .clicked()
                        {
                            clear_filters = true;
                        }
                    });
                } else {
                    toolbar_row(ui, 30.0, |ui| {
                        ui.add_sized(
                            [54.0, 30.0],
                            egui::Label::new(egui::RichText::new("Search").strong()),
                        );
                        let control_width = 132.0;
                        let search_width = (ui.available_width()
                            - 3.0 * control_width
                            - 3.0 * ui.spacing().item_spacing.x
                            - 2.0)
                            .max(240.0);
                        ui.add_sized(
                            [search_width, 30.0],
                            egui::TextEdit::singleline(&mut self.filters.search)
                                .id_salt("global_search")
                                .hint_text(
                                    "key, source, translation, story, quest, owner or speaker",
                                ),
                        );
                        filter_combo_sized(
                            ui,
                            "Status",
                            &mut self.filters.status,
                            status_options,
                            control_width,
                        );
                        filter_combo_sized(
                            ui,
                            "Type",
                            &mut self.filters.entry_type,
                            types.iter().map(String::as_str),
                            control_width,
                        );
                        sort_combo(ui, &mut self.filters.sort_mode, control_width);
                    });
                    ui.add_space(6.0);
                    toolbar_row(ui, 30.0, |ui| {
                        filter_combo_sized(
                            ui,
                            "Story",
                            &mut self.filters.story,
                            stories.iter().map(String::as_str),
                            150.0,
                        );
                        filter_combo_sized(
                            ui,
                            "Quest",
                            &mut self.filters.quest,
                            quests.iter().map(String::as_str),
                            150.0,
                        );
                        filter_combo_sized(
                            ui,
                            "Owner",
                            &mut self.filters.owner,
                            owners.iter().map(String::as_str),
                            150.0,
                        );
                        ui.checkbox(&mut self.filters.only_orphaned, "Orphaned only");
                        if ui
                            .add_sized([112.0, 30.0], egui::Button::new("Clear filters"))
                            .clicked()
                        {
                            clear_filters = true;
                        }
                    });
                }
                if self.filters.is_active() {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(egui::RichText::new("Active filters").strong());
                        if !self.filters.search.is_empty()
                            && ui
                                .small_button(format!("Search: {} ×", self.filters.search))
                                .clicked()
                        {
                            chip_to_clear = Some("Search");
                        }
                        for (label, value) in [
                            ("Status", &self.filters.status),
                            ("Type", &self.filters.entry_type),
                            ("Story", &self.filters.story),
                            ("Quest", &self.filters.quest),
                            ("Owner", &self.filters.owner),
                        ] {
                            if !value.is_empty()
                                && ui.small_button(format!("{label}: {value} ×")).clicked()
                            {
                                chip_to_clear = Some(label);
                            }
                        }
                    });
                }
            });

        if clear_filters {
            self.filters.clear();
        }
        if locale_changed {
            if self.draft_dirty {
                self.locale = previous_locale;
                self.notify(
                    "Save or reset the current draft before changing locale.",
                    true,
                );
            } else {
                if self.locale == self.store.catalog.source_locale {
                    if !self.filters.status.is_empty() && self.filters.status != "source" {
                        self.filters.status.clear();
                    }
                } else if self.filters.status == "source" {
                    self.filters.status.clear();
                }
                self.load_draft();
            }
        }
        if let Some(label) = chip_to_clear {
            match label {
                "Status" => self.filters.status.clear(),
                "Type" => self.filters.entry_type.clear(),
                "Story" => self.filters.story.clear(),
                "Quest" => self.filters.quest.clear(),
                "Owner" => self.filters.owner.clear(),
                "Search" => self.filters.search.clear(),
                _ => {}
            }
        }

        let mut visible_keys = self.store.filtered_keys(&self.locale, &self.filters);
        if self.show_content {
            visible_keys.retain(|key| {
                self.store
                    .catalog
                    .entries
                    .get(key)
                    .map(|metadata| {
                        metadata.entry_type == "dialogue_line"
                            || metadata.entry_type == "quest_description"
                    })
                    .unwrap_or(false)
            });
        }
        self.keyboard_action(context, &visible_keys);
        egui::SidePanel::left("explorer")
            .default_width(EXPLORER_WIDTH)
            .width_range(SIDE_PANEL_MIN_WIDTH..=SIDE_PANEL_MAX_WIDTH)
            .resizable(true)
            .frame(panel_frame(panel_background(), 12.0))
            .show(context, |ui| {
                constrain_side_panel_content(ui);
                ui.horizontal(|ui| {
                    ui.heading(if self.show_content {
                        "Content explorer"
                    } else {
                        "Key registry"
                    });
                    ui.add_space(4.0);
                    ui.weak(format!("{} results", visible_keys.len()));
                });
                ui.separator();
                egui::ScrollArea::vertical()
                    .id_salt("explorer_scroll")
                    .max_width(ui.available_width())
                    .scroll_bar_visibility(
                        egui::containers::scroll_area::ScrollBarVisibility::VisibleWhenNeeded,
                    )
                    .auto_shrink([false, false])
                    .show_rows(ui, 54.0, visible_keys.len(), |ui, row_range| {
                        for index in row_range {
                            let key = &visible_keys[index];
                            let Some(metadata) = self.store.catalog.entries.get(key) else {
                                continue;
                            };
                            let story = metadata.story_id.as_deref().unwrap_or("Unassigned");
                            let status = self.target_status(key).to_string();
                            let status_text = status_display(&status);
                            let can_edit_locale = self.locale != self.store.catalog.source_locale;
                            let row_valid = self.store.validate_entry(&self.locale, key).is_empty();
                            let selected = self.selected.as_deref() == Some(key.as_str());
                            let row_safe = !selected || !self.draft_dirty;
                            let (row_rect, _) = ui.allocate_exact_size(
                                egui::vec2(ui.available_width(), 54.0),
                                egui::Sense::click(),
                            );
                            if selected || ui.rect_contains_pointer(row_rect) {
                                let fill = if selected {
                                    egui::Color32::from_rgb(47, 86, 126)
                                } else {
                                    egui::Color32::from_rgb(38, 47, 59)
                                };
                                ui.painter().rect_filled(row_rect, 5.0, fill);
                            }
                            ui.scope_builder(
                                egui::UiBuilder::new().max_rect(row_rect.shrink(8.0)),
                                |ui| {
                                    ui.with_layout(
                                        egui::Layout::left_to_right(egui::Align::Center),
                                        |ui| {
                                            ui.add_sized(
                                                [18.0, 36.0],
                                                egui::Label::new(
                                                    egui::RichText::new("●")
                                                        .color(status_color(&status)),
                                                ),
                                            );
                                            ui.vertical(|ui| {
                                                ui.add(
                                                    egui::Label::new(
                                                        egui::RichText::new(key).strong(),
                                                    )
                                                    .truncate(),
                                                )
                                                .on_hover_text(key);
                                                ui.add(
                                                    egui::Label::new(format!(
                                                        "{story} · {} · {status_text}",
                                                        metadata.entry_type
                                                    ))
                                                    .truncate(),
                                                );
                                            });
                                        },
                                    );
                                },
                            );
                            let response = ui.interact(
                                row_rect,
                                egui::Id::new(("entry_row", key.as_str())),
                                egui::Sense::click(),
                            );
                            if response.clicked() {
                                self.request_selection(key.clone());
                            }
                            response.context_menu(|ui| {
                                ui.label(key);
                                ui.separator();
                                if ui.button("Open").clicked() {
                                    self.request_selection(key.clone());
                                    ui.close_menu();
                                }
                                if ui.button("Copy key").clicked() {
                                    ui.ctx().copy_text(key.clone());
                                    ui.close_menu();
                                }
                                if ui.button("Copy source text").clicked() {
                                    if let Some(source) = self.store.source.entries.get(key) {
                                        ui.ctx().copy_text(source.text.clone());
                                    }
                                    ui.close_menu();
                                }
                                if ui.button("Copy translation").clicked() {
                                    if let Some(view) = self.store.entry_view(&self.locale, key) {
                                        ui.ctx().copy_text(view.target_text);
                                    }
                                    ui.close_menu();
                                }
                                ui.separator();
                                if ui
                                    .add_enabled(
                                        can_edit_locale && status != WAITING && row_safe,
                                        egui::Button::new("Mark as waiting"),
                                    )
                                    .clicked()
                                {
                                    pending_context_action =
                                        Some((key.clone(), ContextAction::MarkWaiting));
                                    ui.close_menu();
                                }
                                if ui
                                    .add_enabled(
                                        can_edit_locale
                                            && status == WAITING
                                            && row_valid
                                            && row_safe,
                                        egui::Button::new("Mark on review"),
                                    )
                                    .clicked()
                                {
                                    pending_context_action =
                                        Some((key.clone(), ContextAction::MarkOnReview));
                                    ui.close_menu();
                                }
                                if ui
                                    .add_enabled(
                                        can_edit_locale
                                            && status == ON_REVIEW
                                            && row_valid
                                            && row_safe,
                                        egui::Button::new("Accept"),
                                    )
                                    .clicked()
                                {
                                    pending_context_action =
                                        Some((key.clone(), ContextAction::Accept));
                                    ui.close_menu();
                                }
                                if ui
                                    .add_enabled(
                                        can_edit_locale && status != WAITING && row_safe,
                                        egui::Button::new("Reject"),
                                    )
                                    .clicked()
                                {
                                    pending_context_action =
                                        Some((key.clone(), ContextAction::Reject));
                                    ui.close_menu();
                                }
                                if ui.button("Show usages").clicked() {
                                    pending_context_action =
                                        Some((key.clone(), ContextAction::ShowUsages));
                                    ui.close_menu();
                                }
                                if ui.button("Show metadata").clicked() {
                                    pending_context_action =
                                        Some((key.clone(), ContextAction::ShowMetadata));
                                    ui.close_menu();
                                }
                            });
                        }
                    });
            });

        // Side panels must be laid out before the central panel so the editor
        // receives the actual remaining width instead of rendering underneath
        // the inspector on narrow windows.
        if self.show_inspector {
            egui::SidePanel::right("inspector")
                .default_width(INSPECTOR_WIDTH)
                .width_range(SIDE_PANEL_MIN_WIDTH..=SIDE_PANEL_MAX_WIDTH)
                .resizable(true)
                .frame(panel_frame(panel_background(), 12.0))
                .show(context, |ui| {
                    constrain_side_panel_content(ui);
                    ui.heading("Inspector");
                    egui::ScrollArea::vertical()
                        .id_salt("inspector_scroll")
                        .max_width(ui.available_width())
                        .scroll_bar_visibility(
                            egui::containers::scroll_area::ScrollBarVisibility::VisibleWhenNeeded,
                        )
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            if let Some(view) = self.selected_view() {
                                status_label(ui, &view.status);
                                ui.label(&view.key);
                                ui.separator();
                                ui.strong("Validation");
                                let errors = if self.draft_dirty {
                                    self.store
                                        .validation_errors_for_text(&view.key, &self.draft)
                                } else {
                                    view.validation_errors.clone()
                                };
                                if errors.is_empty() {
                                    ui.colored_label(
                                        egui::Color32::from_rgb(92, 195, 124),
                                        "Valid for review",
                                    );
                                } else {
                                    for error in errors {
                                        ui.colored_label(
                                            egui::Color32::from_rgb(235, 150, 120),
                                            format!("• {error}"),
                                        );
                                    }
                                }
                                if view.metadata.orphaned {
                                    ui.colored_label(
                                        egui::Color32::from_rgb(235, 150, 120),
                                        "Orphaned key",
                                    );
                                }
                                ui.separator();
                                ui.strong("Metadata");
                                show_optional_metadata(
                                    ui,
                                    "Story",
                                    view.metadata.story_id.as_deref(),
                                );
                                show_optional_metadata(
                                    ui,
                                    "Quest",
                                    view.metadata.quest_id.as_deref(),
                                );
                                show_optional_metadata(
                                    ui,
                                    "Dialogue",
                                    view.metadata.dialogue_id.as_deref(),
                                );
                                show_optional_metadata(
                                    ui,
                                    "Owner",
                                    view.metadata.owner_id.as_deref(),
                                );
                                show_optional_metadata(
                                    ui,
                                    "Speaker",
                                    view.metadata.speaker_id.as_deref(),
                                );
                                show_optional_metadata(
                                    ui,
                                    "Translator",
                                    view.translated_by.as_deref(),
                                );
                                ui.label(format!("Source hash: {}", short_hash(&view.source_hash)));
                                ui.label(format!(
                                    "Translation hash: {}",
                                    short_hash(&view.text_hash)
                                ));
                                if view.status == ACCEPTED
                                    && view.source_hash != view.stored_source_hash
                                {
                                    ui.colored_label(
                                        egui::Color32::from_rgb(235, 150, 120),
                                        "Source hash is stale",
                                    );
                                }
                                ui.separator();
                                ui.strong("Usages");
                                for usage in usages(&view.metadata) {
                                    ui.label(format!("• {usage}"));
                                }
                                ui.separator();
                                ui.collapsing("Differences", |ui| {
                                    ui.label(format!(
                                        "English: {} characters",
                                        view.source_text.chars().count()
                                    ));
                                    ui.label(format!(
                                        "Translation: {} characters",
                                        self.draft.chars().count()
                                    ));
                                    ui.label(format!(
                                        "English lines: {}",
                                        line_count(&view.source_text)
                                    ));
                                    ui.label(format!(
                                        "Translation lines: {}",
                                        line_count(&self.draft)
                                    ));
                                });
                            } else {
                                ui.label("Select an entry to inspect its metadata and validation.");
                            }
                        });
                });
        }

        egui::CentralPanel::default()
            .frame(panel_frame(app_background(), 16.0))
            .show(context, |ui| {
                if self.selected_view().is_none() {
                    let empty_rect = ui.available_rect_before_wrap();
                    ui.allocate_new_ui(
                        egui::UiBuilder::new()
                            .max_rect(empty_rect)
                            .layout(egui::Layout::top_down(egui::Align::Center)),
                        |ui| {
                            ui.set_width(empty_rect.width());
                            ui.add_space(80.0);
                            ui.label(egui::RichText::new("Select an entry").heading());
                            ui.add(
                                egui::Label::new("Choose an entry from the explorer to begin editing.")
                                    .wrap_mode(egui::TextWrapMode::Wrap)
                                    .halign(egui::Align::Center),
                            );
                        },
                    );
                    return;
                }
                egui::ScrollArea::vertical()
                    .id_salt("editor_scroll")
                    .scroll_bar_visibility(
                        egui::containers::scroll_area::ScrollBarVisibility::VisibleWhenNeeded,
                    )
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        let Some(view) = self.selected_view() else {
                            return;
                        };
                        ui.add(
                            egui::Label::new(egui::RichText::new(&view.key).heading())
                                .wrap_mode(egui::TextWrapMode::Wrap),
                        );
                        ui.horizontal_wrapped(|ui| {
                            status_label(ui, &view.status);
                            if self.draft_dirty {
                                ui.colored_label(egui::Color32::from_rgb(236, 173, 72), "● Unsaved changes");
                            }
                        });
                        ui.label(format!("{} · {}", view.metadata.entry_type, locale_name(&self.locale)));
                        ui.separator();

                        ui.strong("Source (English)");
                        let mut source_text = view.source_text.clone();
                        ui.add(
                            egui::TextEdit::multiline(&mut source_text)
                                .desired_rows(4)
                                .interactive(false)
                                .desired_width(f32::INFINITY),
                        );
                        editor_counts(ui, &view.source_text);

                        if self.locale == self.store.catalog.source_locale {
                            ui.colored_label(
                                egui::Color32::from_rgb(160, 180, 205),
                                "The source locale is read-only in this editor.",
                            );
                        } else if view.metadata.orphaned || view.source_hash.is_empty() {
                            ui.colored_label(
                                egui::Color32::from_rgb(235, 150, 120),
                                "This key is orphaned and cannot be edited until it is mapped to a source entry.",
                            );
                            let mut orphaned_text = self.draft.clone();
                            ui.add(
                                egui::TextEdit::multiline(&mut orphaned_text)
                                    .desired_rows(8)
                                    .desired_width(f32::INFINITY)
                                    .interactive(false),
                            );
                            editor_counts(ui, &self.draft);
                        } else {
                            ui.add_space(10.0);
                            ui.horizontal_wrapped(|ui| {
                                ui.strong(format!("Translation ({})", locale_name(&self.locale)));
                                status_label(ui, &view.status);
                            });
                            let changed = ui
                                .add(
                                    egui::TextEdit::multiline(&mut self.draft)
                                        .desired_rows(12)
                                        .desired_width(f32::INFINITY)
                                        .hint_text("Enter the translation. Variables and commands are validated before review."),
                                )
                                .changed();
                            if changed {
                                self.draft_dirty = true;
                            }
                            editor_counts(ui, &self.draft);
                            let draft_errors = self.store.validation_errors_for_text(&view.key, &self.draft);
                            if draft_errors.is_empty() {
                                ui.colored_label(egui::Color32::from_rgb(92, 195, 124), "Validation passed");
                            } else {
                                ui.colored_label(egui::Color32::from_rgb(235, 102, 102), "Validation needs attention");
                                for error in &draft_errors {
                                    ui.colored_label(egui::Color32::from_rgb(235, 150, 120), format!("• {error}"));
                                }
                            }
                            ui.horizontal_wrapped(|ui| {
                                let can_review = !self.draft_dirty
                                    && draft_errors.is_empty()
                                    && view.status == WAITING;
                                if ui
                                    .add_enabled(
                                        self.draft_dirty,
                                        egui::Button::new("Save").min_size(egui::vec2(92.0, 32.0)),
                                    )
                                    .clicked()
                                {
                                    pending_action = Some(EditorAction::Save);
                                }
                                if ui
                                    .add_enabled(
                                        can_review,
                                        egui::Button::new("Mark on review")
                                            .min_size(egui::vec2(132.0, 32.0)),
                                    )
                                    .clicked()
                                {
                                    pending_action = Some(EditorAction::MarkOnReview);
                                }
                                let can_accept = !self.draft_dirty
                                    && draft_errors.is_empty()
                                    && view.status == ON_REVIEW;
                                if ui
                                    .add_enabled(
                                        can_accept,
                                        egui::Button::new("Accept")
                                            .min_size(egui::vec2(92.0, 32.0)),
                                    )
                                    .clicked()
                                {
                                    pending_action = Some(EditorAction::Accept);
                                }
                                if ui
                                    .add_enabled(
                                        view.status != WAITING,
                                        egui::Button::new("Reject")
                                            .min_size(egui::vec2(92.0, 32.0)),
                                    )
                                    .clicked()
                                {
                                    pending_action = Some(EditorAction::Reject);
                                }
                                if ui
                                    .add_sized([92.0, 32.0], egui::Button::new("Reset"))
                                    .clicked()
                                {
                                    self.confirm_reset = true;
                                }
                            });
                        }

                        ui.separator();
                        ui.collapsing("Structured tokens and commands", |ui| {
                            let commands = commands_in_text(&view.source_text);
                            if view.metadata.variables.is_empty()
                                && view.metadata.tokens.is_empty()
                                && commands.is_empty()
                            {
                                ui.label("No structured tokens or commands registered.");
                            } else {
                                if !view.metadata.variables.is_empty() {
                                    ui.label(format!("Variables: {}", view.metadata.variables.join(", ")));
                                }
                                let token_values: Vec<String> = if view.metadata.tokens.is_empty() {
                                    commands.into_iter().collect()
                                } else {
                                    view.metadata.tokens.clone()
                                };
                                if !token_values.is_empty() {
                                    ui.label(format!("Commands: {}", token_values.join(", ")));
                                }
                                ui.label("These values must remain unchanged in translations.");
                            }
                        });
                        if self.show_metadata {
                            show_metadata(ui, &view);
                        }
                    });
            });

        if let Some((key, action)) = pending_context_action {
            if matches!(
                action,
                ContextAction::ShowUsages | ContextAction::ShowMetadata
            ) {
                self.request_selection(key);
                self.show_inspector = true;
                if action == ContextAction::ShowMetadata {
                    self.show_metadata = true;
                }
            } else {
                self.apply_context_action(&key, action);
            }
        }
        if let Some(action) = pending_action {
            self.apply_action(action);
        }
        if self.confirm_reset {
            egui::Window::new("Reset translation?")
                .collapsible(false)
                .resizable(false)
                .show(context, |ui| {
                    ui.label("This clears the current translation and returns it to waiting for translation.");
                    ui.horizontal(|ui| {
                        if ui.button("Reset").clicked() {
                            self.confirm_reset = false;
                            self.apply_action(EditorAction::Reset);
                        }
                        if ui.button("Cancel").clicked() {
                            self.confirm_reset = false;
                        }
                    });
                });
        }
        if self.external_conflict {
            egui::Window::new("External changes detected")
                .collapsible(false)
                .resizable(false)
                .show(context, |ui| {
                    ui.label("The catalog changed while you had unsaved edits.");
                    ui.horizontal_wrapped(|ui| {
                        if ui.button("Keep my changes").clicked() {
                            self.external_conflict = false;
                            self.external_snapshot = None;
                            self.notify(
                                "Kept local changes. Saving will overwrite the external version.",
                                true,
                            );
                        }
                        if ui.button("Reload external changes").clicked() {
                            if let Some(snapshot) = self.external_snapshot.take() {
                                self.store = snapshot;
                                self.draft_dirty = false;
                                self.load_draft();
                            }
                            self.external_conflict = false;
                            self.notify("External changes reloaded.", false);
                        }
                        if ui.button("Compare").clicked() {
                            self.show_compare = true;
                        }
                    });
                });
        }
        if self.show_compare {
            egui::Window::new("Compare external changes")
                .resizable(true)
                .show(context, |ui| {
                    ui.label("Your local draft");
                    let mut local_draft = self.draft.clone();
                    ui.add(
                        egui::TextEdit::multiline(&mut local_draft)
                            .desired_rows(8)
                            .interactive(false),
                    );
                    ui.separator();
                    ui.label("External translation");
                    let mut external = self
                        .external_snapshot
                        .as_ref()
                        .and_then(|store| {
                            self.selected
                                .as_deref()
                                .and_then(|key| store.entry_view(&self.locale, key))
                        })
                        .map(|view| view.target_text)
                        .unwrap_or_else(|| "No external value available.".into());
                    ui.add(
                        egui::TextEdit::multiline(&mut external)
                            .desired_rows(8)
                            .interactive(false),
                    );
                    if ui.button("Close comparison").clicked() {
                        self.show_compare = false;
                    }
                });
        }
        if let Some(notification) = &self.notification {
            if notification.expires_at > Instant::now() {
                egui::TopBottomPanel::bottom("notification")
                    .frame(panel_frame(toolbar_background(), 8.0))
                    .show(context, |ui| {
                        let color = if notification.is_error {
                            egui::Color32::from_rgb(235, 150, 120)
                        } else {
                            egui::Color32::from_rgb(133, 210, 157)
                        };
                        ui.colored_label(color, &notification.message);
                    });
            } else {
                self.notification = None;
            }
        }
        context.request_repaint_after(Duration::from_millis(250));
    }
}

fn locale_name(locale: &str) -> &'static str {
    match locale {
        "en" => "English",
        "es" => "Español",
        _ => "Other",
    }
}

fn filter_combo_sized<'a, I>(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut String,
    options: I,
    width: f32,
) where
    I: IntoIterator<Item = &'a str>,
{
    // ComboBox::height controls the popup menu height, not the button height.
    // Keep the button in a fixed slot so columns align without forcing short
    // menus to show an unnecessary scrollbar.
    ui.allocate_ui_with_layout(
        egui::vec2(width, 30.0),
        egui::Layout::top_down(egui::Align::Center),
        |ui| {
            egui::ComboBox::from_id_salt(label)
                .width(width)
                .selected_text(if value.is_empty() {
                    format!("All {label}s")
                } else {
                    format!("{label}: {value}")
                })
                .show_ui(ui, |ui| {
                    if ui
                        .selectable_value(value, String::new(), format!("All {label}s"))
                        .changed()
                    {
                        ui.memory_mut(|memory| memory.close_popup());
                    }
                    for option in options {
                        if ui
                            .selectable_value(value, option.to_string(), option)
                            .changed()
                        {
                            ui.memory_mut(|memory| memory.close_popup());
                        }
                    }
                });
        },
    );
}

fn sort_combo(ui: &mut egui::Ui, value: &mut SortMode, width: f32) {
    ui.allocate_ui_with_layout(
        egui::vec2(width, 30.0),
        egui::Layout::top_down(egui::Align::Center),
        |ui| {
            egui::ComboBox::from_id_salt("sort_selector")
                .width(width)
                .selected_text(format!("Sort: {}", value.label()))
                .show_ui(ui, |ui| {
                    for mode in [
                        SortMode::Key,
                        SortMode::Status,
                        SortMode::Story,
                        SortMode::Owner,
                    ] {
                        if ui.selectable_value(value, mode, mode.label()).changed() {
                            ui.memory_mut(|memory| memory.close_popup());
                        }
                    }
                });
        },
    );
}

fn editor_counts(ui: &mut egui::Ui, text: &str) {
    ui.small(format!(
        "{} characters · {} lines",
        text.chars().count(),
        line_count(text)
    ));
}

fn line_count(text: &str) -> usize {
    text.lines().count().max(1)
}

fn truncate_for_node(text: &str, max_chars: usize) -> String {
    let mut chars = text.chars();
    let result: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        format!("{result}…")
    } else {
        result
    }
}

fn short_hash(hash: &str) -> &str {
    hash.get(..12).unwrap_or(hash)
}

fn show_optional_metadata(ui: &mut egui::Ui, label: &str, value: Option<&str>) {
    ui.label(format!("{label}: {}", value.unwrap_or("—")));
}

fn show_metadata(ui: &mut egui::Ui, view: &EntryViewModel) {
    ui.collapsing("Metadata", |ui| {
        show_optional_metadata(ui, "Story", view.metadata.story_id.as_deref());
        show_optional_metadata(ui, "Quest", view.metadata.quest_id.as_deref());
        show_optional_metadata(ui, "Dialogue", view.metadata.dialogue_id.as_deref());
        show_optional_metadata(ui, "Owner", view.metadata.owner_id.as_deref());
        show_optional_metadata(ui, "Speaker", view.metadata.speaker_id.as_deref());
        show_optional_metadata(ui, "Translator", view.translated_by.as_deref());
        ui.label(format!("Source hash: {}", view.source_hash));
        ui.label(format!("Translation hash: {}", view.text_hash));
    });
}

fn usages(metadata: &KeyEntry) -> Vec<String> {
    let mut result = Vec::new();
    if let Some(story) = &metadata.story_id {
        result.push(format!("story: {story}"));
    }
    if let Some(quest) = &metadata.quest_id {
        result.push(format!("quest: {quest}"));
    }
    if let Some(dialogue) = &metadata.dialogue_id {
        result.push(format!("dialogue: {dialogue}"));
    }
    if let Some(owner) = &metadata.owner_id {
        result.push(format!("owner: {owner}"));
    }
    if result.is_empty() {
        result.push("No mapped usages".into());
    }
    result
}
