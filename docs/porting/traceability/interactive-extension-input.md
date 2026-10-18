# components/extension-input.ts

Source: `coding-agent/src/modes/interactive/components/extension-input.ts`
Rust: `crates/pi-coding-agent/src/modes/interactive/components/extension_input.rs`

Tests: `interactive_components_c.rs::extension_input_*`, differential `extensionInput`.

| Symbol / branch | Rust item | Proof |
|---|---|---|
| `ExtensionInputComponent` | `ExtensionInputComponent` | behavioral + differential |
| `constructor(title, placeholder, onSubmit, onCancel, opts)` | `new(title, Option<&str>, Rc<dyn Fn(String)>, Rc<dyn Fn()>, ExtensionInputOptions)` | differential |
| optional countdown title (`timeout`, `tui`) | `CountdownTimer` + `poll_countdown` | title/tick covered by countdown unit behavior |
| `secret` mask | `InputOptions { mask }` | `extension_input_masks_secret_values` |
| confirm (binding or `\n`) submits input value | `Component::handle_input` | `extension_input_submits_and_cancels` |
| cancel callback | `Component::handle_input` | same |
| other keys forwarded to Input | `Component::handle_input` | same |
| `dispose` stops countdown | `dispose` | compile/behavioral |
| focus propagation | `Focusable` impl | API surface |

## Gaps

- The unused `placeholder` argument is accepted and ignored, matching the source.

## Discrepancies

None found.
