# Hide progress messages from transcript views

## Background

Progress messages (`TranscriptEntry::Progress`) make up 42–73% of transcript
lines. They represent streaming noise (hook progress, agent progress) and
clutter both the fullscreen transcript viewer and the detail view transcript
pane, making it hard to follow the actual conversation.

## Goals

- Hide progress messages by default in the fullscreen transcript view.
- Provide a `p` keybinding to toggle progress visibility in the transcript view.
- Hide progress messages unconditionally in the detail view transcript pane.

## Design Decisions

- **Default hidden**: Progress messages are noise for most users. Hiding by
  default dramatically reduces line count and improves readability.
- **Toggle in fullscreen only**: The detail view is a compact preview — no
  toggle needed there. The fullscreen transcript view gets a `p` toggle with
  a bottom-bar hint so users can inspect progress when needed.

## TODO

- [x] Add `show_progress` field to `TranscriptViewState` (default `false`)
- [x] Skip `Progress` entries in `build_lines()` when hidden
- [x] Add `p` keybinding to toggle `show_progress`
- [x] Show toggle hint in transcript title bar
- [x] Skip `Progress` entries in detail view's `render_transcript_pane()`
- [x] Build and test

## Future Work

None anticipated.
