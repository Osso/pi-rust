# `tui/src/editor-component.ts` traceability

Rust: `crates/pi-tui/src/editor_component.rs`.

The source is a structural TypeScript interface with optional members. Rust
models required members as required trait methods and optional members as
default-implemented methods.

| Source member | Rust item | Test |
|---|---|---|
| `extends Component` | `EditorComponent: Component` | `editor_editing.rs` render/input tests through `Editor` |
| `getText()` | `get_text` | accessor tests |
| `setText(text)` | `set_text` | normalization/history tests |
| `handleInput(data)` | inherited `Component::handle_input` | all input tests |
| `onSubmit?` | `Editor::on_submit` (concrete editor field) | submit tests |
| `onChange?` | `Editor::on_change` | change-callback assertions |
| `addToHistory?(text)` | default `add_to_history` | history tests |
| `insertTextAtCursor?(text)` | default `insert_text_at_cursor` | insert-text tests |
| `getExpandedText?()` | default `get_expanded_text` (falls back to `get_text`) | paste expansion tests |
| `setAutocompleteProvider?(provider)` | default `set_autocomplete_provider` | autocomplete tests |
| `borderColor?` | default `set_border_color` | `custom_theme_border_color_is_used` |
| `setPaddingX?(padding)` | default `set_padding_x` | padding tests |
| `setAutocompleteMaxVisible?(maxVisible)` | default `set_autocomplete_max_visible` | differential autocomplete options |
| `setWorking?` / `setWorkingIndicator?` / `setScreenOrigin?` / `clearScreenOrigin?` | default no-ops + `supports_working_prompt` opt-in; `Editor` overrides all four | `crates/pi-coding-agent/tests/interactive_workflows.rs` (working-editor) |

Typed Rust exclusions: callbacks are `Option<Rc<dyn Fn(&str)>>` fields rather
than assignable properties. Optional methods cannot be distinguished from
present-but-default implementations at runtime, so the working-prompt methods
are guarded by the explicit `supports_working_prompt` capability flag. `Editor`
implements every method.
