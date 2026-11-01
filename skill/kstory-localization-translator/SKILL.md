---
name: kstory-localization-translator
description: Translate pending KStory localization entries from a .kstory project into a requested target language and mark valid results as on_review. Do not use for approving translations or changing source content.
metadata:
  short-description: Translate KStory dialogue and quest entries for review
---

# KStory localization translator

Use this skill for translation work in a KStory `.kstory` project. The project manifest
is the only required KStory-specific input; do not assume a repository layout.

## Workflow

1. Read `keys.json`, `dialogues.json`, `quests.json`, the target locale file, and `references/glossary.md`.
2. Select only entries whose target status is `waiting_for_translation`, optionally filtered by locale, story, quest, dialogue, or key.
3. Translate only the natural-language text. Never translate keys, owners, speakers, identifiers, commands, item IDs, graph node IDs, action parameters, condition parameters, or event markers.
4. Preserve variables such as `{item}`, `{entity}`, `{count}`, `{amount}`, line breaks, and intentional punctuation.
5. Keep names and terminology consistent with the glossary and with the existing accepted translations.
6. Validate every translated entry against its source placeholders and metadata before writing it.
7. Write the candidate through `kstory set-translation --project path/to/Game.kstory --locale <locale> --key <key> --text <text> --translated-by <agent>`, preserving the source hash and setting exactly `status: on_review`.
8. Report translated, skipped, and invalid entries. Never set `accepted`; human review owns that transition.

## Safety rules

- Do not edit `locales/en.json`, `keys.json`, dialogue order, quest ownership, or source metadata.
- Do not edit `stories.json`, content graph connections, node types, quest requirements, items, entities, callbacks or any other content structure. Content authoring and translation are separate workflows.
- Do not overwrite an entry that is `on_review` or `accepted` unless the user explicitly requests a correction; corrections return to `waiting_for_translation` first.
- If a command-looking token or placeholder is ambiguous, skip the entry and report it instead of guessing.
- A group request must not approve a group. It may only produce `on_review` candidates.

## Output

End with a compact report containing the target locale, number translated, number skipped, validation failures, and the exact review scope a person can approve.
