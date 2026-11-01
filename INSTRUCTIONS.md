# KStory Instructions

## Requirements

- Stable Rust and Cargo.
- A project manifest with the `.kstory` extension.
- A desktop environment with a system WebView for Dioxus Desktop. On macOS this is the native WebKit WebView hosted by the Rust desktop process.

KStory is maintained for the GitHub user `Maetschl`. The canonical project is a `.kstory`
manifest plus the JSON files and locale directory referenced by it. The manifest itself
contains no content and all paths must be relative to its directory. Schema version `1`
is frozen. The generated export directory is build output, not a second source of truth.

## Running from the repository root

The preferred entry point is an explicit project manifest:

```bash
cargo run -- open path/to/Game.kstory
cargo run -- sync --project path/to/Game.kstory
cargo run -- validate --project path/to/Game.kstory
cargo run -- export --project path/to/Game.kstory
cargo run -- gui --project path/to/Game.kstory
```

For compatibility, a legacy content directory can still be opened while migrating:

```bash
cargo run -- gui --project path/to/localization
```

`open` launches the desktop editor for the selected manifest. `gui` is an alias for
launching the same editor. The GUI synchronizes when it opens and exposes an explicit
`Sync` action for external changes. CLI commands do not initialize the desktop renderer.

The manifest has this shape:

```json
{
  "format_version": 1,
  "content_schema_version": 1,
  "project_id": "game",
  "paths": {
    "keys": "content/keys.json",
    "stories": "content/stories.json",
    "dialogues": "content/dialogues.json",
    "quests": "content/quests.json",
    "items": "content/items.json",
    "entities": "content/entities.json",
    "callbacks": "content/callbacks.json",
    "locales": "content/locales",
    "export": "build/kstory"
  }
}
```

Absolute paths and `..` traversal are rejected. `keys.json` remains the authority for
source and target locales; those settings are not duplicated in the manifest.

## Content authoring

Select `Content studio` to work on the playable content graph. The left explorer lists stories and supports search; the central area has graph, sequence and preview views; the right inspector edits node properties, links and story metadata. `Stories` are the primary graph editor, while `Quests`, `Dialogues`, `Items` and `Entities` expose the related catalogs and stable IDs.

Create a story with `New story`, add nodes from the graph toolbar, select a node to edit its display key and typed payload, then use `Save`. Use `Arrange vertically` for a deterministic top-down graph and `Reader` to follow localized dialogue and choices. Node IDs are technical identities and are not renamed when a visible name key changes. The reader uses the selected locale, shows gameplay commands as separate non-translatable nodes and reports Java callbacks as non-simulatable dependencies.

Dialogue nodes contain text lines only. Gameplay content is represented by one typed command node: `show_item`, `show_icon`, `camera_shake_start`, `camera_shake_stop` or another validated action. A dialogue cannot contain a command line, and a command cannot carry unrelated parameters such as an item ID on a camera-shake node. Connections describe flow, labels and conditions only; they do not contain embedded actions.

Projects may provide their own content migrations as CLI extensions, but KStory itself
does not assume a particular game's NPCs, quests or Java source layout.

Connections have stable IDs and are edited through the command layer. Start a connection from an output handle, then select the target input handle; duplicate edges and invalid targets are rejected. Connection targets and output IDs can also be edited in the inspector, and `Disconnect` removes the edge without changing node IDs. Legacy `next` and `choices` are migrated when loaded and projected back during export for Java compatibility.

The source-of-truth content files are `stories.json`, `dialogues.json`, `quests.json`, `items.json`, `entities.json` and `callbacks.json`. The Java game reads these canonical files directly. `export` still creates `runtime/` projections for packaging or inspection, but they are generated and must not be committed as a second source of truth.

Content validation blocks export for missing references, invalid graph connections, inaccessible nodes, unsupported actions/conditions and unregistered callbacks. Callback definitions document the Java code that still owns a special old-core behaviour.

## GUI workflow

The desktop editor uses a three-panel layout:

- The left content explorer groups entries by story and provides a scrollable, virtual-friendly list for large catalogs.
- The center editor keeps English source text read-only and edits the selected locale as a draft. Save, review, accept, reject and reset are explicit actions.
- The right inspector shows validation, ownership, usages, hashes and source/translation details. It can be resized or hidden for smaller windows.

The second toolbar row combines text search with status, type, story, quest, owner and orphan filters. Search covers keys, source text, translations and ownership metadata. Use `Clear filters` to reset the complete filter state. All visible panels have scroll areas, and the sort menu supports key, status, story and owner ordering.

Right-click an entry for `Open`, copy actions, status transitions, usages and metadata. Invalid transitions are disabled. Useful shortcuts are:

- `Ctrl/Cmd+S`: save the current draft as `waiting_for_translation`.
- `Ctrl/Cmd+Enter`: mark a valid saved translation `on_review`.
- `Ctrl/Cmd+Shift+Enter`: accept a valid `on_review` translation.
- `Ctrl/Cmd+F`: focus the global search field.
- `Arrow Up/Down`: move through the filtered explorer; `Enter` opens the first entry when none is selected.
- `Escape`: close a confirmation or conflict dialog.

The editor reports validation errors before review or acceptance. Variables and line breaks must match the English source. Gameplay commands are validated from typed node payloads and never belong in translated text. Saving a manual edit returns it to `waiting_for_translation`; only a valid `on_review` entry can be accepted.

Selection and locale changes are guarded while a draft has unsaved changes. Save or reset the draft before moving to another entry or locale. The explorer rows are full-width targets, so clicking the key, metadata or status area opens the same entry.

If files change outside the editor, press `Sync` before continuing to edit. Save or export after reviewing the resulting validation message; do not overwrite another operator's uncommitted work.

## Common commands

```bash
# Create or update metadata and pending entries
cargo run -- sync

# Validate statuses, hashes, variables and line breaks
cargo run -- validate

# Generate ignored runtime projections under runtime/
cargo run -- export

# List pending entries
cargo run -- list --locale es --status waiting_for_translation

# Register a translation candidate: it becomes on_review
cargo run -- set-translation \
  --locale es \
  --key dialog.example.line \
  --text "Translated text" \
  --translated-by agent

# Accept a reviewed translation as a human reviewer
cargo run -- accept --locale es --key dialog.example.line

# Accept all on_review entries for a story
cargo run -- accept --locale es --story intro

# Accept all on_review entries for a locale
cargo run -- accept --locale es --all-on-review

# Reject an entry and return it to waiting_for_translation
cargo run -- reject --locale es --key dialog.example.line
```

Only `accepted` entries are exported as active translations. Pending and on-review entries fall back to English.

## Status workflow

```text
waiting_for_translation -> on_review -> accepted
          ^                     |
          +------ reject -------+
```

- `waiting_for_translation`: the translation is missing, the source changed, or the translation was edited manually.
- `on_review`: a skill or agent wrote a valid translation.
- `accepted`: a human reviewer approved the translation.

When source text changes, the tool updates its hash and returns only that key to `waiting_for_translation`. Group approvals skip pending, invalid, or stale-source entries.

## LLM translation skill

The skill is located at `skill/kstory-localization-translator/SKILL.md`. It works through
the selected `.kstory` project, only on pending entries for the requested language,
preserves variables, line breaks and non-translatable commands, and writes through
`set-translation`. The skill must never approve an entry.

## Troubleshooting

- **The game displays English:** run `sync`, `validate` and `export`; verify that the Spanish entry is `accepted`.
- **Validation reports incompatible variables:** preserve exactly the same `{variable}` markers as the source.
- **Validation reports incompatible formatting:** preserve the same number of line breaks.
- **A translation returned to pending:** `sync` detected a manual edit or a source change; regenerate and review the translation.
- **The GUI does not start:** verify Stable Rust and a graphical environment; use `validate` and `export` to work without the GUI.

