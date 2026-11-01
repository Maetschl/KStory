# KStory Content Studio Audit Checklist

This checklist is written in English so it can be used by the content and QA teams on every platform.

## Automated checks

Run these commands from the repository root:

```bash
cargo fmt --all -- --check
cargo test
cargo clippy --all-targets -- -D warnings
cargo run -- validate --project path/to/Game.kstory
cargo run -- export --project path/to/Game.kstory
gradle test # in KStory-LibGDX
```

The audit must cover deterministic top-down layout, stable branch positions, orphan and cycle diagnostics, reader navigation, accepted-locale rendering, English fallback, non-translatable commands, atomic story creation, pending translation keys, stable IDs, reproducible export, and invalid-reference rejection.

## Manual content-studio audit

1. Start the GUI from the repository root:

   ```bash
   cargo run -- open path/to/Game.kstory
   ```

2. Create a new story with **New story**.
3. Confirm that the new story is selected and opens on its initial dialogue node.
4. Confirm that English source keys exist and target locale entries are `waiting_for_translation`.
5. Add several nodes with **New node**.
6. Connect a linear sequence in the inspector.
7. Add a choice node with two outgoing branches.
8. Click **Arrange vertically** and confirm that levels flow downward and branches are separated horizontally.
9. Confirm semantic node colors: dialogue blue, choice violet, condition yellow, action orange, quest nodes green, scene transition teal, event magenta, and end gray.
10. Open **Reader**.
11. Read the story in English and confirm speaker names, commands, items, quests, flags, and simulated actions are visible.
12. Change **Locale** to Spanish.
13. Confirm that accepted Spanish text is shown; otherwise confirm the visible **Fallback to English** warning and its translation status.
14. Select each branch and confirm that the reader advances to the selected node.
15. Use **Back**, choose a different branch, and use **Restart**.
16. Confirm commands such as `[SHOW:HAMMER]` remain unchanged and callbacks are labelled **Java callback required**.
17. Run **Validate** and fix every blocking error.
18. Run **Export** and confirm that only accepted translations are exported.
19. Restart the tool and confirm that content, layout, selection, and locale remain usable.
20. Repeat the audit in a small window, a large window, and with a large catalog. All graph, explorer, reader, and inspector regions must remain scrollable without clipping important controls.

## Failure reporting

Record the platform, commit, catalog path, active locale, story ID, node ID, and exact command or UI action that failed. Never approve a translation as part of this audit; approval remains a separate human review action.
