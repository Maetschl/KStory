use super::*;
use std::collections::{BTreeMap, BTreeSet};

/// Computes a deterministic top-down layout without mutating the catalog.
/// Reachable nodes are placed by graph depth; nodes that are not reachable
/// from the entry node are placed in a separate diagnostics area below it.
pub fn arrange_story_top_down(
    story: &StoryDefinition,
) -> (BTreeMap<String, StoryLayoutPosition>, StoryLayoutIssues) {
    let mut positions = BTreeMap::new();
    let mut levels: BTreeMap<usize, BTreeSet<String>> = BTreeMap::new();
    let mut depth_by_node = BTreeMap::new();
    let mut frontier = vec![story.entry_node_id.clone()];
    depth_by_node.insert(story.entry_node_id.clone(), 0usize);

    while let Some(node_id) = frontier.pop() {
        let depth = depth_by_node[&node_id];
        levels.entry(depth).or_default().insert(node_id.clone());
        let Some(_) = story.nodes.get(&node_id) else {
            continue;
        };
        let mut children = outgoing_node_ids(story, &node_id);
        children.sort();
        children.dedup();
        for child in children.into_iter().rev() {
            if !depth_by_node.contains_key(&child) && story.nodes.contains_key(&child) {
                depth_by_node.insert(child.clone(), depth + 1);
                frontier.push(child);
            }
        }
    }

    let mut max_level = 0usize;
    for (level, nodes) in &levels {
        max_level = max_level.max(*level);
        let total_width = nodes.len() as f32 * NODE_WIDTH
            + nodes.len().saturating_sub(1) as f32 * LAYOUT_HORIZONTAL_GAP;
        let start_x = (720.0 - total_width / 2.0).max(48.0);
        for (index, node_id) in nodes.iter().enumerate() {
            positions.insert(
                node_id.clone(),
                StoryLayoutPosition {
                    x: start_x + index as f32 * (NODE_WIDTH + LAYOUT_HORIZONTAL_GAP),
                    y: 48.0 + *level as f32 * (NODE_HEIGHT + LAYOUT_VERTICAL_GAP),
                },
            );
        }
    }

    let orphaned_nodes: Vec<String> = story
        .nodes
        .keys()
        .filter(|node_id| !depth_by_node.contains_key(*node_id))
        .cloned()
        .collect();
    let orphan_y = 48.0 + (max_level + 1) as f32 * (NODE_HEIGHT + LAYOUT_VERTICAL_GAP);
    for (index, node_id) in orphaned_nodes.iter().enumerate() {
        positions.insert(
            node_id.clone(),
            StoryLayoutPosition {
                x: 48.0 + index as f32 * (NODE_WIDTH + LAYOUT_HORIZONTAL_GAP),
                y: orphan_y,
            },
        );
    }

    let mut cycle_edges = Vec::new();
    for node_id in story.nodes.keys() {
        let Some(source_depth) = depth_by_node.get(node_id) else {
            continue;
        };
        let children = outgoing_node_ids(story, node_id);
        for child in children {
            if depth_by_node
                .get(&child)
                .is_some_and(|target_depth| target_depth <= source_depth)
            {
                cycle_edges.push((node_id.clone(), child));
            }
        }
    }
    cycle_edges.sort();
    cycle_edges.dedup();

    (
        positions,
        StoryLayoutIssues {
            orphaned_nodes,
            cycle_edges,
        },
    )
}

pub fn outgoing_node_ids(story: &StoryDefinition, node_id: &str) -> Vec<String> {
    if !story.connections.is_empty() {
        return story
            .connections
            .values()
            .filter(|connection| connection.from_node_id == node_id)
            .map(|connection| connection.to_node_id.clone())
            .collect();
    }
    story
        .nodes
        .get(node_id)
        .map(|node| {
            node.next
                .iter()
                .cloned()
                .chain(node.choices.iter().map(|choice| choice.next.clone()))
                .collect()
        })
        .unwrap_or_default()
}
