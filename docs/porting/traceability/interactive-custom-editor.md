# components/custom-editor.ts

Source: `coding-agent/src/modes/interactive/components/custom-editor.ts`
Rust: `crates/pi-coding-agent/src/modes/interactive/components/custom_editor.rs`

Tests: `interactive_components_c.rs::custom_editor_*` (key routing, history persistence,
legacy migration, filtering). Differential: rendering skipped (needs a TUI/terminal); history
behavior is process/DB-bound and covered by behavioral tests.

| Symbol / branch | Rust item | Proof |
|---|---|---|
| `CustomEditor extends Editor` | `CustomEditor` wrapper (delegates render/input) | behavior tests |
| `constructor` history load (`readOrMigratePromptHistory`) | `new` -> `load_prompt_history` | persistence test |
| `legacyPromptHistoryPath` JSON array filter/slice(0,100) | `read_legacy_prompt_history` | legacy migration test |
| `promptHistoryFilter` | `PromptHistoryFilter` | filter test |
| `onAction` | `on_action` | interrupt/exit tests |
| `addToHistory` + persist newest when changed | `add_to_history` | persistence test |
| `addToHistoryWithoutPersistence` | `add_to_history_without_persistence` | API surface |
| `onExtensionShortcut` first | `on_extension_shortcut` | API surface |
| `app.clipboard.pasteImage` -> `onPasteImage` | `Component::handle_input` | API surface |
| `app.interrupt` (skip when autocomplete shown) | `Component::handle_input` | interrupt test |
| `app.exit` only when text empty | `Component::handle_input` | exit test |
| history previous/next forwarded before app actions | `Component::handle_input` | code path |
| remaining actions iterate in insertion order, `!== false` | `IndexMap` + `bool` handler | action tests |

## Gaps

- `addToHistory` and the constructor return `Result` where the source throws on control-DB errors.
- App action handlers return `bool` (`true` = handled); the source treats any value other than
  `false` as handled.
- Rendering is delegated to the ported `Editor`; no differential case because the source
  requires a terminal-backed `TUI`.

## Discrepancies

None found.
