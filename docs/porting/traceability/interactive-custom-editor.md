# components/custom-editor.ts

## Native representation checkpoint

Editor text/history, prompt-history paths/filter, shortcut input and action map keys, render/input and EditorComponent callbacks use `JsString`. Legacy history files decode at IO and parse with `parse_lossless`; no early scalar authority. Frozen source differential for high/low/pair values covers in-memory history, input and extension-shortcut callback units (`raw_extension_dialogs`). Actual CustomEditor/Editor modules run with no DB path; database functions are uncalled dependency fixtures, so this is not persistence/migration proof. Private real-file legacy JSON test covers raw strings/non-string filtering. The pre-existing void EditorComponent history adapter still discards persistence errors; propagation is not proven by this representation checkpoint.

Source: `coding-agent/src/modes/interactive/components/custom-editor.ts`
Rust: `crates/pi-coding-agent/src/modes/interactive/components/custom_editor.rs`

Tests: `interactive_components_c.rs::custom_editor_*` (key routing, history persistence,
legacy migration, filtering). Rendering differential now runs through the [native editor examples](extension-examples-editors.md) with real pinned CustomEditor/Editor/TUI; history behavior is process/DB-bound and covered by behavioral tests.

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
| callbacks run while the editor is live | `collect_input_callbacks` returns matched callbacks; `EditorHandle::handle_input` runs them after the `RefCell` borrow drops | `interactive_mode_starts_the_tui_and_exits_on_double_ctrl_c` (ctrl+c clear re-enters the editor) |

## Gaps

- `addToHistory` and the constructor return `Result` where the source throws on control-DB errors.
- `onExtensionShortcut` still runs inline (it reports whether it consumed the key); it is not
  wired by `InteractiveMode` yet, so it cannot re-enter the editor.
- App action handlers return `bool` (`true` = handled); the source treats any value other than
  `false` as handled.
- Rendering delegates to the ported `Editor`; example differentials provide a private terminal-backed TUI.

## Discrepancies

None found.

## Factory-built editors

`CustomEditor::as_focusable_mut` exposes its existing Focusable implementation. The interactive `AnyEditorHandle` forwards focus through the editor component. Factory-built real editors receive input, render the cursor marker, and inherit submit callbacks (`extension_ui_factory_builds_real_custom_editor`).
