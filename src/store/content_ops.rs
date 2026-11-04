use super::*;

impl Store {
    pub(crate) fn create_story(&mut self) -> Result<CreatedStory, AppError> {
        let story_id = next_content_id("story", self.content.stories.keys());
        let dialogue_id = format!("dialogue.{story_id}.intro");
        let entry_node_id = format!("{story_id}.start");
        let line_id = format!("{dialogue_id}.line.start");
        let name_key = format!("story.{story_id}.name");
        let description_key = format!("story.{story_id}.description");
        let chapter_key = format!("story.{story_id}.chapter.main");
        let node_key = format!("story.{story_id}.node.start");
        let line_key = format!("dialog.{story_id}.intro.start");
        let snapshot = (
            self.catalog.clone(),
            self.source.clone(),
            self.targets.clone(),
            self.content.clone(),
        );

        let result = (|| {
            self.add_key_in_memory(
                &name_key,
                "New story".into(),
                "story_name".into(),
                Some(story_id.clone()),
                Some("content.author".into()),
            )?;
            self.add_key_in_memory(
                &description_key,
                "A new story.".into(),
                "story_description".into(),
                Some(story_id.clone()),
                Some("content.author".into()),
            )?;
            self.add_key_in_memory(
                &chapter_key,
                "Main".into(),
                "chapter_name".into(),
                Some(story_id.clone()),
                Some("content.author".into()),
            )?;
            self.add_key_in_memory(
                &node_key,
                "Start".into(),
                "story_node_name".into(),
                Some(story_id.clone()),
                Some("content.author".into()),
            )?;
            self.add_key_in_memory(
                &line_key,
                "This is the beginning of a new story.".into(),
                "dialogue_line".into(),
                Some(story_id.clone()),
                Some("content.author".into()),
            )?;
            self.content.dialogues.insert(
                dialogue_id.clone(),
                content::DialogueDefinition {
                    name_key: name_key.clone(),
                    story_id: Some(story_id.clone()),
                    chapter_id: Some("main".into()),
                    owner_id: Some("content.author".into()),
                    lines: vec![content::DialogueLineDefinition {
                        id: line_id.clone(),
                        kind: "text".into(),
                        key: Some(line_key),
                        command: None,
                        speaker: Some("Narrator".into()),
                        variables: Vec::new(),
                        on_enter: None,
                        on_exit: None,
                    }],
                    on_enter: Vec::new(),
                    on_exit: Vec::new(),
                },
            );
            self.content.stories.insert(
                story_id.clone(),
                content::StoryDefinition {
                    name_key,
                    description_key,
                    owner_id: "content.author".into(),
                    entry_node_id: entry_node_id.clone(),
                    chapters: vec![content::ChapterDefinition {
                        id: "main".into(),
                        name_key: chapter_key,
                        node_ids: vec![entry_node_id.clone()],
                    }],
                    nodes: BTreeMap::from([(
                        entry_node_id.clone(),
                        content::StoryNode {
                            node_type: "dialogue".into(),
                            name_key: node_key,
                            payload: Some(content::NodePayload::Dialogue {
                                dialogue_id: dialogue_id.clone(),
                                line_id: line_id.clone(),
                            }),
                            chapter_id: Some("main".into()),
                            dialogue_id: Some(dialogue_id.clone()),
                            dialogue_line_id: Some(line_id.clone()),
                            quest_id: None,
                            scene_id: None,
                            event_id: None,
                            action: None,
                            condition: None,
                            choices: Vec::new(),
                            next: Vec::new(),
                            position: [420.0, 48.0],
                        },
                    )]),
                    connections: BTreeMap::new(),
                },
            );
            self.save()?;
            Ok(CreatedStory {
                story_id: story_id.clone(),
                entry_node_id: entry_node_id.clone(),
                dialogue_id: dialogue_id.clone(),
            })
        })();

        if result.is_err() {
            self.catalog = snapshot.0;
            self.source = snapshot.1;
            self.targets = snapshot.2;
            self.content = snapshot.3;
        }
        result
    }

    /// Split the seven blacksmith QuestDialog scenes into independently executable
    /// authoring stories. The dialogue IDs and line IDs remain unchanged so the
    /// legacy Java flow keeps resolving the same text.
    pub(crate) fn migrate_quest_dialog_scenes(&mut self) -> Result<Vec<String>, AppError> {
        const SCENES: &[(&str, &str, &str)] = &[
            (
                "npc_blacksmith",
                "Blacksmith introduction",
                "npc_blacksmith",
            ),
            ("nice_lamp", "The lamp", "nice_lamp"),
            (
                "save_blacksmith_01",
                "Rescue the blacksmith",
                "save_blacksmith_01",
            ),
            (
                "blacksmith_city_help",
                "Help in the city",
                "blacksmith_city_help",
            ),
            (
                "blacksmith_missing_hammer",
                "The wrong hammer",
                "blacksmith_missing_hammer",
            ),
            (
                "blacksmith_found_hammer",
                "The blacksmith's hammer",
                "blacksmith_found_hammer",
            ),
            ("blacksmith_idle", "Blacksmith idle", "blacksmith_idle"),
        ];

        let snapshot = (
            self.catalog.clone(),
            self.source.clone(),
            self.targets.clone(),
            self.content.clone(),
        );
        let result = (|| {
            let mut created = Vec::new();
            for (dialogue_id, display_name, story_suffix) in SCENES {
                let story_id = format!("blacksmith.{story_suffix}");
                if self.content.stories.contains_key(&story_id) {
                    continue;
                }
                let dialogue = self
                    .content
                    .dialogues
                    .get(*dialogue_id)
                    .cloned()
                    .ok_or_else(|| {
                        AppError::Validation(format!(
                            "cannot migrate QuestDialog scene {dialogue_id}: dialogue is missing"
                        ))
                    })?;
                let owner_id = dialogue
                    .owner_id
                    .clone()
                    .unwrap_or_else(|| "npc.blacksmith".into());
                let name_key = format!("ui.story.{story_id}.name");
                let description_key = format!("ui.story.{story_id}.description");
                let chapter_key = format!("ui.story.{story_id}.chapter.main");
                self.add_key_in_memory(
                    &name_key,
                    (*display_name).into(),
                    "story_name".into(),
                    Some(story_id.clone()),
                    Some(owner_id.clone()),
                )?;
                self.add_key_in_memory(
                    &description_key,
                    format!("QuestDialog scene {dialogue_id}."),
                    "story_description".into(),
                    Some(story_id.clone()),
                    Some(owner_id.clone()),
                )?;
                self.add_key_in_memory(
                    &chapter_key,
                    "Main".into(),
                    "chapter_name".into(),
                    Some(story_id.clone()),
                    Some(owner_id.clone()),
                )?;

                let mut nodes = BTreeMap::new();
                let mut node_ids = Vec::new();
                for (index, line) in dialogue.lines.iter().enumerate() {
                    let node_id = format!("{story_id}.line_{index}");
                    let command = line.command.as_deref().unwrap_or_default();
                    let (node_type, payload, dialogue_ref, line_ref, action) =
                        if line.kind == "command" {
                            let (node_type, payload) =
                                command_node_definition(command, &self.content.items);
                            let action = match &payload {
                                content::NodePayload::Action(action) => Some(action.clone()),
                                _ => None,
                            };
                            (node_type, Some(payload), None, None, action)
                        } else {
                            (
                                "dialogue".into(),
                                Some(content::NodePayload::Dialogue {
                                    dialogue_id: (*dialogue_id).into(),
                                    line_id: line.id.clone(),
                                }),
                                Some((*dialogue_id).into()),
                                Some(line.id.clone()),
                                None,
                            )
                        };
                    let node_name_key = if line.kind == "text" {
                        line.key
                            .clone()
                            .unwrap_or_else(|| format!("{story_id}.line_{index}"))
                    } else {
                        let key = format!("ui.story.{story_id}.line_{index}.name");
                        self.add_key_in_memory(
                            &key,
                            command.to_string(),
                            "story_node_name".into(),
                            Some(story_id.clone()),
                            Some(owner_id.clone()),
                        )?;
                        key
                    };
                    nodes.insert(
                        node_id.clone(),
                        content::StoryNode {
                            node_type,
                            name_key: node_name_key,
                            payload,
                            chapter_id: Some("main".into()),
                            dialogue_id: dialogue_ref,
                            dialogue_line_id: line_ref,
                            quest_id: None,
                            scene_id: None,
                            event_id: None,
                            action,
                            condition: None,
                            choices: Vec::new(),
                            next: Vec::new(),
                            position: [420.0, 80.0 + index as f32 * 164.0],
                        },
                    );
                    node_ids.push(node_id);
                }

                let mut connections = BTreeMap::new();
                for index in 0..node_ids.len().saturating_sub(1) {
                    let id = format!("edge.{story_id}.line_{index}");
                    connections.insert(
                        id.clone(),
                        content::ConnectionDefinition {
                            id,
                            from_node_id: node_ids[index].clone(),
                            to_node_id: node_ids[index + 1].clone(),
                            output_id: "default".into(),
                            connection_kind: "next".into(),
                            label_key: None,
                            condition: None,
                        },
                    );
                }
                self.content.stories.insert(
                    story_id.clone(),
                    content::StoryDefinition {
                        name_key,
                        description_key,
                        owner_id: owner_id.clone(),
                        entry_node_id: node_ids.first().cloned().unwrap_or_default(),
                        chapters: vec![content::ChapterDefinition {
                            id: "main".into(),
                            name_key: chapter_key,
                            node_ids,
                        }],
                        nodes,
                        connections,
                    },
                );
                if let Some(dialogue) = self.content.dialogues.get_mut(*dialogue_id) {
                    dialogue.story_id = Some(story_id.clone());
                    dialogue.chapter_id = Some("main".into());
                }
                for metadata in self.catalog.entries.values_mut() {
                    if metadata.dialogue_id.as_deref() == Some(*dialogue_id) {
                        metadata.story_id = Some(story_id.clone());
                    }
                }
                created.push(story_id);
            }
            let blacksmith_story_ids = [
                "blacksmith.npc_blacksmith",
                "blacksmith.nice_lamp",
                "blacksmith.save_blacksmith_01",
                "blacksmith.blacksmith_city_help",
                "blacksmith.blacksmith_missing_hammer",
                "blacksmith.blacksmith_found_hammer",
                "blacksmith.blacksmith_idle",
            ];
            let blacksmith_is_dialogue_only = self
                .content
                .stories
                .get("blacksmith")
                .map(|story| {
                    story.nodes.values().all(|node| {
                        node.dialogue_id.as_deref().is_some_and(|dialogue_id| {
                            SCENES
                                .iter()
                                .any(|(scene_id, _, _)| *scene_id == dialogue_id)
                        })
                    })
                })
                .unwrap_or(false);
            if blacksmith_is_dialogue_only
                && blacksmith_story_ids
                    .iter()
                    .all(|story_id| self.content.stories.contains_key(*story_id))
            {
                self.content.stories.remove("blacksmith");
            }
            self.content
                .save(&self.root)
                .map_err(AppError::Validation)?;
            self.save()?;
            Ok(created)
        })();
        if result.is_err() {
            self.catalog = snapshot.0;
            self.source = snapshot.1;
            self.targets = snapshot.2;
            self.content = snapshot.3;
        }
        result
    }

    /// Split the six opening/gameplay QuestDialog scenes into independent stories.
    ///
    /// These dialogs are not one continuous story in the game: each one is
    /// started by a different scene or interaction. The dialogue and line IDs
    /// remain unchanged so the legacy Java flow keeps resolving the same text,
    /// while the authoring model gets one executable story per QuestDialog.
    pub(crate) fn migrate_opening_quest_dialog_scenes(&mut self) -> Result<Vec<String>, AppError> {
        const SCENES: &[(&str, &str, &str)] = &[
            ("init_text_0", "Opening: the earthquake", "story.intro"),
            ("init_text_1", "Opening: the lamp", "story.intro"),
            ("init_pre_board", "Before the quest board", "story.intro"),
            ("init_board", "Quest board", "story.workbench"),
            (
                "after_workbench_repair",
                "After workbench repair",
                "story.workbench",
            ),
            ("well_repair_quest", "Well repair", "story.well"),
        ];

        let snapshot = (
            self.catalog.clone(),
            self.source.clone(),
            self.targets.clone(),
            self.content.clone(),
        );
        let result = (|| {
            let mut created = Vec::new();

            for (dialogue_id, display_name, owner_id) in SCENES {
                let story_id = format!("quest_dialog.{dialogue_id}");

                if self.content.stories.contains_key(&story_id) {
                    if let Some(dialogue) = self.content.dialogues.get_mut(*dialogue_id) {
                        dialogue.story_id = Some(story_id.clone());
                        dialogue.chapter_id = Some("main".into());
                    }
                    continue;
                }

                let dialogue = self
                    .content
                    .dialogues
                    .get(*dialogue_id)
                    .cloned()
                    .ok_or_else(|| {
                        AppError::Validation(format!(
                            "cannot migrate QuestDialog scene {dialogue_id}: dialogue is missing"
                        ))
                    })?;
                let name_key = format!("ui.story.{story_id}.name");
                let description_key = format!("ui.story.{story_id}.description");
                let chapter_key = format!("ui.story.{story_id}.chapter.main");

                self.add_key_in_memory(
                    &name_key,
                    (*display_name).into(),
                    "story_name".into(),
                    Some(story_id.clone()),
                    Some((*owner_id).into()),
                )?;
                self.add_key_in_memory(
                    &description_key,
                    format!("QuestDialog scene {dialogue_id}."),
                    "story_description".into(),
                    Some(story_id.clone()),
                    Some((*owner_id).into()),
                )?;
                self.add_key_in_memory(
                    &chapter_key,
                    "Main".into(),
                    "chapter_name".into(),
                    Some(story_id.clone()),
                    Some((*owner_id).into()),
                )?;

                let mut nodes = BTreeMap::new();
                let mut node_ids = Vec::new();
                for (index, line) in dialogue.lines.iter().enumerate() {
                    let node_id = format!("{story_id}.line_{index}");
                    let command = line.command.as_deref().unwrap_or_default();
                    let (node_type, payload, dialogue_ref, line_ref, action) =
                        if line.kind == "command" {
                            let (node_type, payload) =
                                command_node_definition(command, &self.content.items);
                            let action = match &payload {
                                content::NodePayload::Action(action) => Some(action.clone()),
                                _ => None,
                            };
                            (node_type, Some(payload), None, None, action)
                        } else {
                            (
                                "dialogue".into(),
                                Some(content::NodePayload::Dialogue {
                                    dialogue_id: (*dialogue_id).into(),
                                    line_id: line.id.clone(),
                                }),
                                Some((*dialogue_id).into()),
                                Some(line.id.clone()),
                                None,
                            )
                        };
                    let node_name_key = if line.kind == "text" {
                        line.key
                            .clone()
                            .unwrap_or_else(|| format!("{story_id}.line_{index}"))
                    } else {
                        let key = format!("ui.story.{story_id}.line_{index}.name");
                        self.add_key_in_memory(
                            &key,
                            command.to_string(),
                            "story_node_name".into(),
                            Some(story_id.clone()),
                            Some((*owner_id).into()),
                        )?;
                        key
                    };

                    nodes.insert(
                        node_id.clone(),
                        content::StoryNode {
                            node_type,
                            name_key: node_name_key,
                            payload,
                            chapter_id: Some("main".into()),
                            dialogue_id: dialogue_ref,
                            dialogue_line_id: line_ref,
                            quest_id: None,
                            scene_id: None,
                            event_id: None,
                            action,
                            condition: None,
                            choices: Vec::new(),
                            next: Vec::new(),
                            position: [420.0, 80.0 + index as f32 * 164.0],
                        },
                    );
                    node_ids.push(node_id);
                }

                let mut connections = BTreeMap::new();
                for index in 0..node_ids.len().saturating_sub(1) {
                    let id = format!("edge.{story_id}.line_{index}");
                    connections.insert(
                        id.clone(),
                        content::ConnectionDefinition {
                            id,
                            from_node_id: node_ids[index].clone(),
                            to_node_id: node_ids[index + 1].clone(),
                            output_id: "default".into(),
                            connection_kind: "next".into(),
                            label_key: None,
                            condition: None,
                        },
                    );
                }

                self.content.stories.insert(
                    story_id.clone(),
                    content::StoryDefinition {
                        name_key,
                        description_key,
                        owner_id: (*owner_id).into(),
                        entry_node_id: node_ids.first().cloned().unwrap_or_default(),
                        chapters: vec![content::ChapterDefinition {
                            id: "main".into(),
                            name_key: chapter_key,
                            node_ids,
                        }],
                        nodes,
                        connections,
                    },
                );
                if let Some(dialogue) = self.content.dialogues.get_mut(*dialogue_id) {
                    dialogue.story_id = Some(story_id.clone());
                    dialogue.chapter_id = Some("main".into());
                }
                for metadata in self.catalog.entries.values_mut() {
                    if metadata.dialogue_id.as_deref() == Some(*dialogue_id) {
                        metadata.story_id = Some(story_id.clone());
                    }
                }
                created.push(story_id);
            }

            // Remove the same dialogue scenes from their old grouped stories.
            // Keep workbench's quest nodes because they are separate gameplay
            // content, not part of the dialogue scenes being split here.
            for (old_story_id, old_dialogues) in [
                (
                    "intro",
                    &["init_text_0", "init_text_1", "init_pre_board"][..],
                ),
                ("workbench", &["init_board", "after_workbench_repair"][..]),
                ("well", &["well_repair_quest"][..]),
            ] {
                let Some(story) = self.content.stories.get_mut(old_story_id) else {
                    continue;
                };
                let removed_nodes: BTreeSet<String> = story
                    .nodes
                    .iter()
                    .filter(|(_, node)| {
                        node.dialogue_id
                            .as_deref()
                            .map(|dialogue_id| old_dialogues.contains(&dialogue_id))
                            .unwrap_or(false)
                    })
                    .map(|(node_id, _)| node_id.clone())
                    .collect();
                for node_id in &removed_nodes {
                    story.nodes.remove(node_id);
                }
                story.connections.retain(|_, connection| {
                    !removed_nodes.contains(&connection.from_node_id)
                        && !removed_nodes.contains(&connection.to_node_id)
                });
                for chapter in &mut story.chapters {
                    chapter
                        .node_ids
                        .retain(|node_id| !removed_nodes.contains(node_id));
                }
                if removed_nodes.contains(&story.entry_node_id) {
                    story.entry_node_id = story
                        .chapters
                        .iter()
                        .flat_map(|chapter| chapter.node_ids.iter())
                        .next()
                        .cloned()
                        .unwrap_or_default();
                }
            }

            self.content.stories.retain(|story_id, story| {
                !(matches!(story_id.as_str(), "intro" | "well") && story.nodes.is_empty())
            });
            self.content
                .save(&self.root)
                .map_err(AppError::Validation)?;
            self.save()?;
            Ok(created)
        })();

        if result.is_err() {
            self.catalog = snapshot.0;
            self.source = snapshot.1;
            self.targets = snapshot.2;
            self.content = snapshot.3;
        }
        result
    }

    pub(crate) fn create_independent_dialogue_story(
        &mut self,
        dialogue_id: &str,
        display_name: &str,
        story_id: &str,
        owner_id: &str,
    ) -> Result<bool, AppError> {
        if self.content.stories.contains_key(story_id) {
            if let Some(dialogue) = self.content.dialogues.get_mut(dialogue_id) {
                dialogue.story_id = Some(story_id.into());
                dialogue.chapter_id = Some("main".into());
            }
            return Ok(false);
        }

        let dialogue = self
            .content
            .dialogues
            .get(dialogue_id)
            .cloned()
            .ok_or_else(|| {
                AppError::Validation(format!(
                    "cannot migrate QuestDialog scene {dialogue_id}: dialogue is missing"
                ))
            })?;
        let name_key = format!("ui.story.{story_id}.name");
        let description_key = format!("ui.story.{story_id}.description");
        let chapter_key = format!("ui.story.{story_id}.chapter.main");
        self.add_key_in_memory(
            &name_key,
            display_name.into(),
            "story_name".into(),
            Some(story_id.into()),
            Some(owner_id.into()),
        )?;
        self.add_key_in_memory(
            &description_key,
            format!("QuestDialog scene {dialogue_id}."),
            "story_description".into(),
            Some(story_id.into()),
            Some(owner_id.into()),
        )?;
        self.add_key_in_memory(
            &chapter_key,
            "Main".into(),
            "chapter_name".into(),
            Some(story_id.into()),
            Some(owner_id.into()),
        )?;

        let mut nodes = BTreeMap::new();
        let mut node_ids = Vec::new();
        for (index, line) in dialogue.lines.iter().enumerate() {
            let node_id = format!("{story_id}.line_{index}");
            let command = line.command.as_deref().unwrap_or_default();
            let (node_type, payload, dialogue_ref, line_ref, action) = if line.kind == "command" {
                let (node_type, payload) = command_node_definition(command, &self.content.items);
                let action = match &payload {
                    content::NodePayload::Action(action) => Some(action.clone()),
                    _ => None,
                };
                (node_type, Some(payload), None, None, action)
            } else {
                (
                    "dialogue".into(),
                    Some(content::NodePayload::Dialogue {
                        dialogue_id: dialogue_id.into(),
                        line_id: line.id.clone(),
                    }),
                    Some(dialogue_id.into()),
                    Some(line.id.clone()),
                    None,
                )
            };
            let node_name_key = if line.kind == "text" {
                line.key
                    .clone()
                    .unwrap_or_else(|| format!("{story_id}.line_{index}"))
            } else {
                let key = format!("ui.story.{story_id}.line_{index}.name");
                self.add_key_in_memory(
                    &key,
                    command.to_string(),
                    "story_node_name".into(),
                    Some(story_id.into()),
                    Some(owner_id.into()),
                )?;
                key
            };
            nodes.insert(
                node_id.clone(),
                content::StoryNode {
                    node_type,
                    name_key: node_name_key,
                    payload,
                    chapter_id: Some("main".into()),
                    dialogue_id: dialogue_ref,
                    dialogue_line_id: line_ref,
                    quest_id: None,
                    scene_id: None,
                    event_id: None,
                    action,
                    condition: None,
                    choices: Vec::new(),
                    next: Vec::new(),
                    position: [420.0, 80.0 + index as f32 * 164.0],
                },
            );
            node_ids.push(node_id);
        }
        let mut connections = BTreeMap::new();
        for index in 0..node_ids.len().saturating_sub(1) {
            let id = format!("edge.{story_id}.line_{index}");
            connections.insert(
                id.clone(),
                content::ConnectionDefinition {
                    id,
                    from_node_id: node_ids[index].clone(),
                    to_node_id: node_ids[index + 1].clone(),
                    output_id: "default".into(),
                    connection_kind: "next".into(),
                    label_key: None,
                    condition: None,
                },
            );
        }
        self.content.stories.insert(
            story_id.into(),
            content::StoryDefinition {
                name_key,
                description_key,
                owner_id: owner_id.into(),
                entry_node_id: node_ids.first().cloned().unwrap_or_default(),
                chapters: vec![content::ChapterDefinition {
                    id: "main".into(),
                    name_key: chapter_key,
                    node_ids,
                }],
                nodes,
                connections,
            },
        );
        if let Some(dialogue) = self.content.dialogues.get_mut(dialogue_id) {
            dialogue.story_id = Some(story_id.into());
            dialogue.chapter_id = Some("main".into());
        }
        for metadata in self.catalog.entries.values_mut() {
            if metadata.dialogue_id.as_deref() == Some(dialogue_id) {
                metadata.story_id = Some(story_id.into());
            }
        }
        Ok(true)
    }

    pub(crate) fn migrate_remaining_grouped_dialogue_scenes(
        &mut self,
    ) -> Result<Vec<String>, AppError> {
        // Blacksmith already has independent scenes; this call also removes
        // the obsolete blacksmith container once all seven are present.
        self.migrate_quest_dialog_scenes()?;
        const SCENES: &[(&str, &str, &str, &str)] = &[
            (
                "farming_0",
                "Farming: introduction",
                "story.farming",
                "farming.farming_0",
            ),
            (
                "farming_1",
                "Farming: first plot",
                "story.farming",
                "farming.farming_1",
            ),
            (
                "farming_2",
                "Farming: seeds",
                "story.farming",
                "farming.farming_2",
            ),
            (
                "farming_3",
                "Farming: quest",
                "story.farming",
                "farming.farming_3",
            ),
            (
                "farming_4",
                "Farming: follow-up",
                "story.farming",
                "farming.farming_4",
            ),
            (
                "farming_5",
                "Farming: cleared plots",
                "story.farming",
                "farming.farming_5",
            ),
            (
                "farming_bucket",
                "Farming: bucket",
                "story.farming",
                "farming.farming_bucket",
            ),
            (
                "npc_default",
                "Generic NPC: default",
                "story.generic_npc",
                "generic_npc.npc_default",
            ),
            (
                "oldman_scary",
                "Old man: first meeting",
                "story.oldman",
                "oldman.oldman_scary",
            ),
            (
                "oldman_idle",
                "Old man: idle",
                "story.oldman",
                "oldman.oldman_idle",
            ),
            (
                "oldman_city",
                "Old man: city",
                "story.oldman",
                "oldman.oldman_city",
            ),
            (
                "oldman_shrine_quest",
                "Old man: shrine quest",
                "story.oldman",
                "oldman.oldman_shrine_quest",
            ),
        ];
        let snapshot = (
            self.catalog.clone(),
            self.source.clone(),
            self.targets.clone(),
            self.content.clone(),
        );
        let result = (|| {
            let mut created = Vec::new();
            for (dialogue_id, display_name, owner_id, story_id) in SCENES {
                if self.create_independent_dialogue_story(
                    dialogue_id,
                    display_name,
                    story_id,
                    owner_id,
                )? {
                    created.push((*story_id).to_string());
                }
            }

            for (old_story_id, old_dialogues) in [
                (
                    "farming",
                    &[
                        "farming_0",
                        "farming_1",
                        "farming_2",
                        "farming_3",
                        "farming_4",
                        "farming_5",
                        "farming_bucket",
                    ][..],
                ),
                ("generic_npc", &["npc_default", "test_dialog"][..]),
                (
                    "oldman",
                    &[
                        "oldman_scary",
                        "oldman_idle",
                        "oldman_city",
                        "oldman_shrine_quest",
                    ][..],
                ),
            ] {
                let Some(story) = self.content.stories.get_mut(old_story_id) else {
                    continue;
                };
                let removed_nodes: BTreeSet<String> = story
                    .nodes
                    .iter()
                    .filter(|(_, node)| {
                        node.dialogue_id
                            .as_deref()
                            .map(|dialogue_id| old_dialogues.contains(&dialogue_id))
                            .unwrap_or(false)
                    })
                    .map(|(node_id, _)| node_id.clone())
                    .collect();
                for node_id in &removed_nodes {
                    story.nodes.remove(node_id);
                }
                story.connections.retain(|_, connection| {
                    !removed_nodes.contains(&connection.from_node_id)
                        && !removed_nodes.contains(&connection.to_node_id)
                });
                for chapter in &mut story.chapters {
                    chapter
                        .node_ids
                        .retain(|node_id| !removed_nodes.contains(node_id));
                }
            }

            // Split the remaining quest-only nodes too. The old grouped
            // stories are not part of the authoring format anymore; each
            // executable piece gets its own story while keeping its node ID.
            for (old_story_id, node_id, story_id, display_name) in [
                (
                    "blacksmith",
                    "blacksmith.quest_hammer",
                    "blacksmith.quest_hammer",
                    "Craft the hammer",
                ),
                (
                    "blacksmith",
                    "blacksmith.quest_house",
                    "blacksmith.quest_house",
                    "Repair the house",
                ),
                (
                    "blacksmith",
                    "blacksmith.quest_save",
                    "blacksmith.quest_save",
                    "Save the blacksmith",
                ),
                (
                    "farming",
                    "farming.quest_craft_bucket",
                    "farming.quest_craft_bucket",
                    "Craft the bucket",
                ),
                (
                    "farming",
                    "farming.quest_farming",
                    "farming.quest_farming",
                    "Farming quest",
                ),
                (
                    "farming",
                    "farming.quest_shovel_pots",
                    "farming.quest_shovel_pots",
                    "Farming: shovel plots",
                ),
                ("oldman", "oldman.quest", "oldman.quest", "Old man quest"),
                (
                    "workbench",
                    "workbench.quest_craft_hammer",
                    "workbench.quest_craft_hammer",
                    "Craft the hammer",
                ),
                (
                    "workbench",
                    "workbench.quest_repair_entity",
                    "workbench.quest_repair_entity",
                    "Repair the workbench",
                ),
            ] {
                if self.content.stories.contains_key(story_id) {
                    continue;
                }
                let (mut node, owner_id) =
                    if let Some(source_story) = self.content.stories.get(old_story_id) {
                        let Some(node) = source_story.nodes.get(node_id).cloned() else {
                            continue;
                        };
                        (node, source_story.owner_id.clone())
                    } else if matches!(
                        old_story_id,
                        "blacksmith" | "farming" | "oldman" | "workbench"
                    ) {
                        let quest_id = match node_id {
                            "blacksmith.quest_hammer" => "npc_hammer",
                            "blacksmith.quest_house" => "repair_house",
                            "blacksmith.quest_save" => "save_blacksmith",
                            "farming.quest_craft_bucket" => "craft_bucket",
                            "farming.quest_farming" => "farming",
                            "farming.quest_shovel_pots" => "shovel_pots",
                            "oldman.quest" => "oldman_shrine",
                            "workbench.quest_craft_hammer" => "craft_hammer",
                            "workbench.quest_repair_entity" => "repair_entity",
                            _ => continue,
                        };
                        (
                            content::StoryNode {
                                node_type: "quest_start".into(),
                                name_key: String::new(),
                                payload: Some(content::NodePayload::QuestStart {
                                    quest_id: quest_id.into(),
                                }),
                                chapter_id: None,
                                dialogue_id: None,
                                dialogue_line_id: None,
                                quest_id: Some(quest_id.into()),
                                scene_id: None,
                                event_id: None,
                                action: None,
                                condition: None,
                                choices: Vec::new(),
                                next: Vec::new(),
                                position: [420.0, 80.0],
                            },
                            "story.blacksmith".into(),
                        )
                    } else {
                        continue;
                    };
                let name_key = format!("ui.story.{story_id}.name");
                let description_key = format!("ui.story.{story_id}.description");
                let chapter_key = format!("ui.story.{story_id}.chapter.main");
                self.add_key_in_memory(
                    &name_key,
                    display_name.into(),
                    "story_name".into(),
                    Some(story_id.into()),
                    Some(owner_id.clone()),
                )?;
                self.add_key_in_memory(
                    &description_key,
                    format!("Standalone content story for {node_id}."),
                    "story_description".into(),
                    Some(story_id.into()),
                    Some(owner_id.clone()),
                )?;
                self.add_key_in_memory(
                    &chapter_key,
                    "Main".into(),
                    "chapter_name".into(),
                    Some(story_id.into()),
                    Some(owner_id.clone()),
                )?;
                node.chapter_id = Some("main".into());
                self.content.stories.insert(
                    story_id.into(),
                    content::StoryDefinition {
                        name_key,
                        description_key,
                        owner_id,
                        entry_node_id: node_id.into(),
                        chapters: vec![content::ChapterDefinition {
                            id: "main".into(),
                            name_key: chapter_key,
                            node_ids: vec![node_id.into()],
                        }],
                        nodes: BTreeMap::from([(node_id.into(), node)]),
                        connections: BTreeMap::new(),
                    },
                );
                created.push(story_id.to_string());
            }
            for (quest_id, story_id) in [
                ("craft_bucket", "farming.quest_craft_bucket"),
                ("farming", "farming.quest_farming"),
                ("shovel_pots", "farming.quest_shovel_pots"),
                ("npc_hammer", "blacksmith.quest_hammer"),
                ("repair_house", "blacksmith.quest_house"),
                ("save_blacksmith", "blacksmith.quest_save"),
                ("oldman_shrine", "oldman.quest"),
                ("craft_hammer", "workbench.quest_craft_hammer"),
                ("repair_entity", "workbench.quest_repair_entity"),
            ] {
                if let Some(quest) = self.content.quests.get_mut(quest_id) {
                    quest.story_id = Some(story_id.into());
                    quest.chapter_id = Some("main".into());
                }
            }
            for old_story_id in [
                "blacksmith",
                "farming",
                "generic_npc",
                "oldman",
                "workbench",
            ] {
                self.content.stories.remove(old_story_id);
            }
            self.content
                .save(&self.root)
                .map_err(AppError::Validation)?;
            self.save()?;
            Ok(created)
        })();
        if result.is_err() {
            self.catalog = snapshot.0;
            self.source = snapshot.1;
            self.targets = snapshot.2;
            self.content = snapshot.3;
        }
        result
    }

    pub(crate) fn create_node(
        &mut self,
        story_id: &str,
        node_type: &str,
        speaker: &str,
        source_text: &str,
        action_type: Option<&str>,
        auto_connect_from: Option<(String, String, String)>,
    ) -> Result<CreatedNode, AppError> {
        let snapshot = (
            self.catalog.clone(),
            self.source.clone(),
            self.targets.clone(),
            self.content.clone(),
        );
        let result = (|| {
            let (chapter_id, owner_id) = {
                let story = self
                    .content
                    .stories
                    .get(story_id)
                    .ok_or_else(|| AppError::Validation(format!("unknown story {story_id}")))?;
                (
                    story
                        .chapters
                        .first()
                        .map(|chapter| chapter.id.clone())
                        .unwrap_or_else(|| "main".into()),
                    if story.owner_id.is_empty() {
                        "content.author".into()
                    } else {
                        story.owner_id.clone()
                    },
                )
            };
            let node_id = next_content_id(
                &format!("{story_id}.node"),
                self.content.stories[story_id].nodes.keys(),
            );
            let short_id = node_id
                .strip_prefix(&format!("{story_id}."))
                .unwrap_or(node_id.as_str())
                .replace('.', "_");
            let name_key = format!("story.{story_id}.node.{short_id}.name");
            self.add_key_in_memory(
                &name_key,
                format!("New {} node", node_type.replace('_', " ")),
                "story_node_name".into(),
                Some(story_id.into()),
                Some(owner_id.clone()),
            )?;

            let mut dialogue_id = None;
            let mut dialogue_line_id = None;
            let mut line_key = None;
            if node_type == "dialogue" {
                let created_dialogue_id = format!("dialogue.{story_id}.{short_id}");
                let created_line_key = format!("dialog.{story_id}.{short_id}.line");
                let line_id = format!("{created_dialogue_id}.line");
                self.add_key_in_memory(
                    &created_line_key,
                    source_text.to_string(),
                    "dialogue_line".into(),
                    Some(story_id.into()),
                    Some(owner_id.clone()),
                )?;
                self.content.dialogues.insert(
                    created_dialogue_id.clone(),
                    content::DialogueDefinition {
                        name_key: name_key.clone(),
                        story_id: Some(story_id.into()),
                        chapter_id: Some(chapter_id.clone()),
                        owner_id: Some(owner_id.clone()),
                        lines: vec![content::DialogueLineDefinition {
                            id: line_id,
                            kind: "text".into(),
                            key: Some(created_line_key.clone()),
                            command: None,
                            speaker: Some(speaker.into()),
                            variables: placeholders(source_text).into_iter().collect(),
                            on_enter: None,
                            on_exit: None,
                        }],
                        on_enter: Vec::new(),
                        on_exit: Vec::new(),
                    },
                );
                dialogue_id = Some(created_dialogue_id);
                dialogue_line_id = Some(format!("dialogue.{story_id}.{short_id}.line"));
                line_key = Some(created_line_key);
            }

            let typed_action = if node_type == "action" {
                content::action_from_type(action_type.unwrap_or("camera_shake_start"))
            } else {
                None
            };
            if node_type == "action" && typed_action.is_none() {
                return Err(AppError::Validation(format!(
                    "unsupported typed action {}",
                    action_type.unwrap_or("<missing>")
                )));
            }
            if node_type == "show_item" {
                let item_id = action_type.unwrap_or_default();
                if item_id.is_empty() || !self.content.items.contains_key(item_id) {
                    return Err(AppError::Validation(format!(
                        "show_item requires an existing item, got {item_id}"
                    )));
                }
            }
            let payload = if node_type == "dialogue" {
                Some(content::NodePayload::Dialogue {
                    dialogue_id: dialogue_id.clone().unwrap_or_default(),
                    line_id: dialogue_line_id.clone().unwrap_or_default(),
                })
            } else if node_type == "action" {
                typed_action.clone().map(content::NodePayload::Action)
            } else if node_type == "show_item" {
                Some(content::NodePayload::ShowItem {
                    item_id: action_type.unwrap_or("HAMMER").into(),
                })
            } else if node_type == "show_icon" {
                Some(content::NodePayload::ShowIcon {
                    icon_id: action_type.unwrap_or("LOVE").into(),
                })
            } else if node_type == "condition" {
                Some(content::NodePayload::Condition(
                    content::ConditionDefinition {
                        condition_type: "always".into(),
                        params: Default::default(),
                        callback_id: None,
                    },
                ))
            } else if node_type == "end" {
                Some(content::NodePayload::End)
            } else {
                None
            };
            let node = content::StoryNode {
                node_type: node_type.into(),
                name_key,
                chapter_id: Some(chapter_id.clone()),
                dialogue_id: dialogue_id.clone(),
                dialogue_line_id: dialogue_line_id.clone(),
                quest_id: None,
                scene_id: None,
                event_id: None,
                action: typed_action.clone(),
                payload,
                condition: (node_type == "condition").then(|| content::ConditionDefinition {
                    condition_type: "always".into(),
                    params: Default::default(),
                    callback_id: None,
                }),
                choices: Vec::new(),
                next: Vec::new(),
                position: [120.0, 220.0],
            };
            let story = self.content.stories.get_mut(story_id).unwrap();
            story.nodes.insert(node_id.clone(), node);
            if let Some(chapter) = story
                .chapters
                .iter_mut()
                .find(|chapter| chapter.id == chapter_id)
            {
                chapter.node_ids.push(node_id.clone());
            } else {
                story.chapters.push(content::ChapterDefinition {
                    id: chapter_id,
                    name_key: format!("story.{story_id}.chapter.main"),
                    node_ids: vec![node_id.clone()],
                });
            }

            let connection_id =
                if let Some((from_node_id, output_id, connection_kind)) = auto_connect_from {
                    Some(self.connect_nodes(
                        story_id,
                        ConnectionRequest {
                            from_node_id,
                            to_node_id: node_id.clone(),
                            output_id,
                            connection_kind,
                            label_key: None,
                            condition: None,
                        },
                    )?)
                } else {
                    None
                };
            Ok(CreatedNode {
                story_id: story_id.into(),
                node_id,
                dialogue_id,
                line_key,
                connection_id,
            })
        })();
        if result.is_err() {
            self.catalog = snapshot.0;
            self.source = snapshot.1;
            self.targets = snapshot.2;
            self.content = snapshot.3;
        }
        result
    }

    pub(crate) fn connect_nodes(
        &mut self,
        story_id: &str,
        request: ConnectionRequest,
    ) -> Result<String, AppError> {
        let story = self
            .content
            .stories
            .get_mut(story_id)
            .ok_or_else(|| AppError::Validation(format!("unknown story {story_id}")))?;
        if !story.nodes.contains_key(&request.from_node_id) {
            return Err(AppError::Validation(format!(
                "unknown source node {}",
                request.from_node_id
            )));
        }
        if !story.nodes.contains_key(&request.to_node_id) {
            return Err(AppError::Validation(format!(
                "unknown target node {}",
                request.to_node_id
            )));
        }
        if story.nodes[&request.from_node_id].node_type == "end" {
            return Err(AppError::Validation(
                "end nodes cannot have outgoing connections".into(),
            ));
        }
        if story.connections.values().any(|connection| {
            connection.from_node_id == request.from_node_id
                && connection.output_id == request.output_id
                && connection.to_node_id == request.to_node_id
        }) {
            return Err(AppError::Validation("duplicate connection".into()));
        }
        let connection_id = next_content_id("edge", story.connections.keys());
        story.connections.insert(
            connection_id.clone(),
            content::ConnectionDefinition {
                id: connection_id.clone(),
                from_node_id: request.from_node_id,
                to_node_id: request.to_node_id,
                output_id: request.output_id,
                connection_kind: request.connection_kind,
                label_key: request.label_key,
                condition: request.condition,
            },
        );
        Ok(connection_id)
    }

    pub(crate) fn update_connection(
        &mut self,
        story_id: &str,
        connection_id: &str,
        request: ConnectionRequest,
    ) -> Result<(), AppError> {
        let story = self
            .content
            .stories
            .get_mut(story_id)
            .ok_or_else(|| AppError::Validation(format!("unknown story {story_id}")))?;
        if !story.nodes.contains_key(&request.from_node_id)
            || !story.nodes.contains_key(&request.to_node_id)
        {
            return Err(AppError::Validation(
                "connection references an unknown node".into(),
            ));
        }
        if story.connections.values().any(|connection| {
            connection.id != connection_id
                && connection.from_node_id == request.from_node_id
                && connection.output_id == request.output_id
                && connection.to_node_id == request.to_node_id
        }) {
            return Err(AppError::Validation("duplicate connection".into()));
        }
        let connection = story
            .connections
            .get_mut(connection_id)
            .ok_or_else(|| AppError::Validation(format!("unknown connection {connection_id}")))?;
        connection.from_node_id = request.from_node_id;
        connection.to_node_id = request.to_node_id;
        connection.output_id = request.output_id;
        connection.connection_kind = request.connection_kind;
        connection.label_key = request.label_key;
        connection.condition = request.condition;
        Ok(())
    }

    pub(crate) fn delete_connection(
        &mut self,
        story_id: &str,
        connection_id: &str,
    ) -> Result<(), AppError> {
        let story = self
            .content
            .stories
            .get_mut(story_id)
            .ok_or_else(|| AppError::Validation(format!("unknown story {story_id}")))?;
        if story.connections.remove(connection_id).is_none() {
            return Err(AppError::Validation(format!(
                "unknown connection {connection_id}"
            )));
        }
        Ok(())
    }

    #[allow(dead_code, reason = "node deletion is exposed by the legacy editor")]
    pub(crate) fn delete_node(&mut self, story_id: &str, node_id: &str) -> Result<(), AppError> {
        let story = self
            .content
            .stories
            .get_mut(story_id)
            .ok_or_else(|| AppError::Validation(format!("unknown story {story_id}")))?;
        if story.entry_node_id == node_id {
            return Err(AppError::Validation(
                "the story entry node cannot be deleted".into(),
            ));
        }
        if story.nodes.remove(node_id).is_none() {
            return Err(AppError::Validation(format!("unknown node {node_id}")));
        }
        story.connections.retain(|_, connection| {
            connection.from_node_id != node_id && connection.to_node_id != node_id
        });
        for chapter in &mut story.chapters {
            chapter.node_ids.retain(|id| id != node_id);
        }
        Ok(())
    }

    pub(crate) fn update_node(
        &mut self,
        story_id: &str,
        node_id: &str,
        name_key: String,
        node_type: String,
    ) -> Result<(), AppError> {
        if ![
            "dialogue",
            "choice",
            "condition",
            "action",
            "quest_start",
            "quest_update",
            "scene_transition",
            "event",
            "end",
        ]
        .contains(&node_type.as_str())
        {
            return Err(AppError::Validation(format!(
                "unknown node type {node_type}"
            )));
        }
        let story = self
            .content
            .stories
            .get_mut(story_id)
            .ok_or_else(|| AppError::Validation(format!("unknown story {story_id}")))?;
        let node = story
            .nodes
            .get_mut(node_id)
            .ok_or_else(|| AppError::Validation(format!("unknown node {node_id}")))?;
        if let Some(payload) = &node.payload {
            let compatible = matches!(
                (node_type.as_str(), payload),
                ("dialogue", content::NodePayload::Dialogue { .. })
                    | ("show_item", content::NodePayload::ShowItem { .. })
                    | ("show_icon", content::NodePayload::ShowIcon { .. })
                    | ("choice", content::NodePayload::Choice)
                    | ("condition", content::NodePayload::Condition(_))
                    | ("action", content::NodePayload::Action(_))
                    | ("quest_start", content::NodePayload::QuestStart { .. })
                    | ("quest_update", content::NodePayload::QuestUpdate { .. })
                    | (
                        "scene_transition",
                        content::NodePayload::SceneTransition { .. }
                    )
                    | ("event", content::NodePayload::Event { .. })
                    | ("end", content::NodePayload::End)
            );
            if !compatible {
                return Err(AppError::Validation(
                    "node type cannot change without recreating its typed payload".into(),
                ));
            }
        }
        node.name_key = name_key;
        node.node_type = node_type;
        Ok(())
    }

    pub(crate) fn rename_key(&mut self, old_key: &str, new_key: &str) -> Result<(), AppError> {
        if !self.source.entries.contains_key(old_key) {
            return Err(AppError::Validation(format!("unknown key {old_key}")));
        }
        if old_key == new_key {
            return Err(AppError::Validation(
                "new key must differ from old key".into(),
            ));
        }
        if self.source.entries.contains_key(new_key) {
            return Err(AppError::Validation(format!(
                "key already exists: {new_key}"
            )));
        }
        let snapshot = (
            self.catalog.clone(),
            self.source.clone(),
            self.targets.clone(),
            self.content.clone(),
        );
        let source = self.source.entries.remove(old_key).unwrap();
        self.source.entries.insert(new_key.to_string(), source);
        let metadata = self
            .catalog
            .entries
            .remove(old_key)
            .unwrap_or_else(|| KeyEntry {
                entry_type: infer_type(new_key),
                ..Default::default()
            });
        self.catalog.entries.insert(new_key.to_string(), metadata);
        for file in self.targets.values_mut() {
            if let Some(entry) = file.entries.remove(old_key) {
                file.entries.insert(new_key.to_string(), entry);
            }
        }

        let replace = |value: &mut String| {
            if value == old_key {
                *value = new_key.to_string();
            }
        };
        for story in self.content.stories.values_mut() {
            replace(&mut story.name_key);
            replace(&mut story.description_key);
            for chapter in &mut story.chapters {
                replace(&mut chapter.name_key);
            }
            for node in story.nodes.values_mut() {
                replace(&mut node.name_key);
                for choice in &mut node.choices {
                    replace(&mut choice.label_key);
                }
            }
        }
        for dialogue in self.content.dialogues.values_mut() {
            replace(&mut dialogue.name_key);
            for line in &mut dialogue.lines {
                if let Some(key) = &mut line.key {
                    replace(key);
                }
            }
        }
        for quest in self.content.quests.values_mut() {
            replace(&mut quest.name_key);
            for key in &mut quest.description_keys {
                replace(key);
            }
            for stage in &mut quest.stages {
                replace(&mut stage.name_key);
                replace(&mut stage.description_key);
            }
        }
        for item in self.content.items.values_mut() {
            replace(&mut item.name_key);
            replace(&mut item.description_key);
        }
        for entity in self.content.entities.values_mut() {
            replace(&mut entity.name_key);
        }

        if let Err(error) = self.validate() {
            self.catalog = snapshot.0;
            self.source = snapshot.1;
            self.targets = snapshot.2;
            self.content = snapshot.3;
            return Err(error);
        }
        if let Err(error) = self.save_content().and_then(|_| self.save()) {
            self.catalog = snapshot.0;
            self.source = snapshot.1;
            self.targets = snapshot.2;
            self.content = snapshot.3;
            return Err(error);
        }
        Ok(())
    }
}
