# KStory content authoring

`kstory` is a portable localization editor and declarative content authoring tool. Content is versioned as JSON so a story can be reviewed, diffed and exported without opening the game. Projects are opened through a `.kstory` manifest.

## Start the editor

Run these commands from the repository root:

```bash
cargo run -- open path/to/Game.kstory
cargo run -- validate --project path/to/Game.kstory
cargo run -- export --project path/to/Game.kstory
```

The catalog is in the paths declared by the selected `.kstory` manifest.

## Author a story

1. Open `Content studio` and select `Stories`.
2. Choose `New story` to create a complete story seed. The generated story, dialogue and node IDs are stable technical identifiers, and all new translation keys start as `waiting_for_translation`.
3. Use `New node` to create a complete graph node. Select a node to change its node type, name key, linked dialogue or quest, and outgoing connections. New dialogue nodes receive a stable dialogue ID, line ID, source key and pending target-locale entries atomically.
4. Use `Graph` for topology, `Arrange vertically` for a top-down layout, `Sequence` for a readable linear path from the entry node, and `Reader` to inspect localized text, choices and simulated actions.
5. Edit visible names through localization keys. Do not rename IDs after content has been referenced.
6. Save the content catalog, then run `validate` and `export`.

The graph supports dialogue, choice, condition, action, quest start/update, scene transition, event and end nodes. Connections are stored by node ID, never by array position. Reordering a list therefore does not invalidate a translation or a gameplay reference.

Connections are first-class content. Each edge has a stable ID, source and target node, output/choice ID, kind, optional label, condition and actions. Use the output handle and then the target handle to create one. The inspector can change the target or output, and `Disconnect` deletes only the selected edge. The editor rejects duplicate source/output/target combinations and validates missing nodes before export.

## Content catalogs

- `stories.json`: ordered chapters and graph nodes.
- `dialogues.json`: named sequences with stable line IDs, speakers, text keys, commands and actions.
- `quests.json`: names, descriptions, stages, requirements and lifecycle actions.
- `items.json`: logical item definitions and references to existing sprites.
- `entities.json`: narrative NPC/interactable definitions and registered visual entity references.
- `callbacks.json`: explicit Java escape hatches for special legacy logic.

Actions and conditions use typed names and structured parameters. Declarative actions include quest changes, inventory changes, flags, events, scene changes, entity state changes, camera shake and gameplay button enabling. A `callback` is valid only when its `callback_id` is registered in `callbacks.json` and implemented in the Java runtime.

## Preview and runtime

Preview never mutates a real save or game state. It follows the entry node, displays the selected locale, keeps command lines separate from natural language and stops with a clear warning when a Java callback cannot be simulated.

`export` writes generated localization bundles and `runtime/content/` projections for packaging or inspection. The Java runtime reads the canonical catalog directly; generated projections are ignored and are never a second source of truth. Existing old-core scenes remain compatible and continue to own behavior that has not yet been migrated.

## Translation workflow

Names, descriptions and dialogue lines are ordinary localization keys. When a source key changes, its translations return to `waiting_for_translation`. The translator skill may write a valid candidate as `on_review`; a human must accept it individually, by story or globally. It must not change story structure, IDs, ownership, ordering, actions or conditions.

Commands, variables and tokens such as `[SHOW:HAMMER]` and `{Player}` are structured/non-translatable data and must remain identical between source and translation.

## Troubleshooting

- Run `sync` after adding or changing source keys.
- Run `validate` before `export`; it reports the exact story, node, quest, action or callback that blocks export.
- If a story is not visible in the game, run `validate` and confirm that the canonical files under `localization/` are included in the LibGDX assets.
- If an old-core flow still requires custom Java behavior, define a callback instead of hiding logic in an editor string.
