# components/extension-input.ts

Source: `coding-agent/src/modes/interactive/components/extension-input.ts`
Rust: `crates/pi-coding-agent-interactive/src/modes/interactive/components/extension_input.rs`

Tests: `interactive_components_c.rs::extension_input_*`, differential `extensionInput`.

| Symbol / branch | Rust item | Proof |
|---|---|---|
| `ExtensionInputComponent` | `ExtensionInputComponent` | behavioral + differential |
| `constructor(title, placeholder, onSubmit, onCancel, opts)` | `new(&JsString, Option<&JsString>, Rc<dyn Fn(JsString)>, Rc<dyn Fn()>, ExtensionInputOptions)` | differential |
| optional countdown title (`timeout`, `tui`) | `CountdownTimer` + `poll_countdown` | title/tick covered by countdown unit behavior |
| `secret` mask | `InputOptions { mask }` | `extension_input_masks_secret_values` |
| confirm (binding or `\n`) submits input value | `Component::handle_input` | `extension_input_submits_and_cancels` |
| cancel callback | `Component::handle_input` | same |
| other keys forwarded to Input | `Component::handle_input` | same |
| `dispose` stops countdown | `dispose` | compile/behavioral |
| focus propagation | `Focusable` impl | API surface |

## Common-pin native proof

`raw_extension_dialogs.rs` exercises lone high/low units and an astral pair in titles, typed input and submission. `raw-coding-tui-oracle.ts` runs frozen modules and compares native callbacks plus narrow/wide renders before/after invalidation (3 payloads × 3 widths × 2 snapshots). Actual production input/selector/border modules run in the isolated harness; indexed-zero theme is a dependency fixture, so SGR is stripped only after rendering. Countdown raw-title proof remains pending.

## Gaps

- The unused `placeholder` argument is accepted and ignored, matching the source.

## Discrepancies

None found.
