# working-editor.ts

Source: `coding-agent/src/modes/interactive/working-editor.ts`
Rust: `crates/pi-coding-agent/src/modes/interactive/working_editor.rs`

| Symbol / branch | Rust item | Proof |
|---|---|---|
| `hasWorkingPromptMethods` presence check | `EditorComponent::supports_working_prompt` (default `false`) | `interactive_workflows::working_editor_leaves_arbitrary_custom_editors_unchanged` |
| `workingIndicatorFitsPrompt` | `working_indicator_fits_prompt` | `working_editor_rejects_wrong_width_indicators` |
| `supportsWorkingPromptAnimation` | `supports_working_prompt_animation` | same test |
| non-fitting indicator → `setWorking(false)` | `sync_working_editor` early branch | same test (working stays false, indicator unchanged) |
| indicator + working sync | `sync_working_editor` | `working_editor_syncs_indicator_state_and_origin` |
| `positionWorkingEditor` | `position_working_editor` | same test |
| `clearWorkingEditorPosition` | `clear_working_editor_position` | same test |
| `clearWorkingEditor` | `clear_working_editor` | same test |

Representation: TypeScript detects the four optional methods with `typeof`; Rust models them as default no-op `EditorComponent` methods plus an explicit `supports_working_prompt` opt-in (`Editor` returns `true`). The `CustomEditor` inheritance case from the TS test is not ported here because `custom-editor.ts` belongs to ca-interactive-a.
