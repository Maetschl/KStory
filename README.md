# KStory

Portable Rust/Dioxus story authoring and localization editor. KStory is maintained and
published by the GitHub user [Maetschl](https://github.com/Maetschl) and is designed to
work with any game that implements the KStory schema.

Open a project manifest from this directory:

```bash
cargo run -- open path/to/Game.kstory
```

The manifest is the portable project boundary. It contains only project metadata and
safe paths relative to the `.kstory` file; content remains in the referenced JSON files.
Schema version `1` is frozen until an explicit migration is introduced. A legacy content
directory is still accepted for migration and tests with `--project` (or the compatibility
alias `--root`).

```bash
cargo run -- sync --project path/to/Game.kstory
cargo run -- validate --project path/to/Game.kstory
cargo run -- export --project path/to/Game.kstory
cargo run -- list --project path/to/Game.kstory --locale es --status waiting_for_translation
cargo run -- gui --project path/to/Game.kstory
```

The published repositories are owned by `Maetschl`:

- [Maetschl/KStory](https://github.com/Maetschl/KStory): this editor, CLI, schema and translation skill.
- [Maetschl/KStory-LibGDX](https://github.com/Maetschl/KStory-LibGDX): the generic Java/Gradle runtime.

For the complete operator workflow, manifest format, status transitions, the LLM skill and troubleshooting, see [INSTRUCTIONS.md](INSTRUCTIONS.md).
