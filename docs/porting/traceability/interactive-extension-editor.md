# components/extension-editor.ts

Source: `coding-agent/src/modes/interactive/components/extension-editor.ts`
Rust: `crates/pi-coding-agent/src/modes/interactive/components/extension_editor.rs`

Tests: `interactive_components_c.rs::extension_editor_*`, differential `extensionEditor`.

| Symbol / branch | Rust item | Proof |
|---|---|---|
| `ExtensionEditorComponent` | `ExtensionEditorComponent` | behavioral + differential |
| `constructor(tui, keybindings, title, prefill, onSubmit, onCancel, options, externalEditorCommand?)` | `new(Rc<dyn ExtensionEditorHost>, KeybindingsManager, ...)` | differential |
| default external command (`VISUAL`, `EDITOR`, platform default) | constructor resolution | not differential (env-dependent) |
| editor theme | `get_editor_theme()` | differential |
| Enter submits via `editor.onSubmit` | `Editor::on_submit` | `extension_editor_submits_typed_text` |
| cancel binding calls `onCancel` | `Component::handle_input` | `extension_editor_cancels_on_escape` |
| `app.editor.external` opens external editor | `handle_open_external_editor` | OS boundary; not unit-tested |
| focus propagation | `Focusable` impl | API surface |
| hints | `key_hint` | differential |

## Common-pin native checkpoint

Title, prefill, editor content, submission callbacks, rendering/input and external-editor command storage use `JsString`. `raw_extension_dialogs::extension_editor_preserves_raw_prefill_and_submission` exercises lone high/low units and an astral pair through the actual Editor and callback; isolated harness supplies a plain editor theme. Full theme and external-key dispatch integration remain pending.

`external_editor.rs` retains native command/content/result/error values. UTF-8 encoding occurs at actual prompt-file/stdout/process boundaries; saved file bytes decode back to native text. `raw_external_editor` launches Python in the mandatory sandbox, captures prompt bytes and writes two newlines: the port removes exactly one. Three raw payloads pass. This is process/filesystem proof, not full frozen external-editor differential or all failure-branch proof.

## Gaps

- The source suspends/starts the `TUI` around the external editor. The Rust port defines
  `ExtensionEditorHost` (extends `EditorHost`) with `suspend`/`resume`/`request_forced_render`
  implemented for `Rc<TuiMainScreen>` and `Rc<TuiRenderer>`.
- External-editor launch is synchronous in the Rust port; the source awaits a promise.

## Discrepancies

None found.
