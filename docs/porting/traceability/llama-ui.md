# llama UI traceability

Pinned source: `packages/coding-agent/src/extensions/llama/ui.ts` at
`532f7d12cd19235081bd4d6e425bde185652d86f`.
Rust: `crates/pi-coding-agent/src/bundled_extensions/llama/ui.rs`.

Differential: `crates/pi-coding-agent/tests/differential_llama_ui.rs` drives
`oracle/llama-ui-oracle.ts` and compares `runWithProgress` outcomes, progress
updates, and cancel calls for complete/error/cancel/not-stop scenarios.
Behavioral: `crates/pi-coding-agent/tests/llama_ui.rs` renders and drives the
component; `tests/llama_index.rs` runs `showLlamaUi` through a fake
`ExtensionUiContext` factory.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `ui.ts:16` | Download sentinel row value | `DOWNLOAD_VALUE`; download-row test |
| `ui.ts:18-24` | Manager action / progress state shapes | `LlamaManagerAction`, `ProgressState` |
| `ui.ts:26-43` | `contextLabel`: `n_ctx`/`n_ctx_train`, truthiness, ctx args, `k` rounding | `context_label`; context helper test |
| `ui.ts:45-53` | `modelDescription`: loaded/sleeping/status + context | `model_description`; render tests |
| `ui.ts:55-64` | Select-list theme mapping | `select_theme`; render tests |
| `ui.ts:66-78` | `frame` border/title/body/footer | `frame`; render tests |
| `ui.ts:88-98` | `LlamaUi` interface | `LlamaUi`; differential/behavioral fakes |
| `ui.ts:100-105` | `compactCount` (M/k thresholds) | `compact_count`; search render (`1.2k downloads`) |
| `ui.ts:107-249` | `HuggingFaceSearch`: debounce 500ms, cache, fuzzy filter, windowed rows, exact `owner/repo[:quant]` confirm, cancel | `SearchState` + `handle_search_input`/`schedule_search`/`run_search`; debounce/cache test |
| `ui.ts:251-517` | `LlamaView`: content/focus state, model sort, select/confirm/connection error, search, status, progress bar, cancel interception, width truncation | `LlamaView`; render/select/progress/search tests |
| `ui.ts:519-538` | `showLlamaUi`: custom factory, run task, error notification + `done` | `show_llama_ui`/`show_llama_ui_with_context`; TUI flow test |
| `ui.ts:540-...` | `runWithProgress`: settle/cancel race, confirm gate, abort + await settled, error propagation | `run_with_progress`; 4 differential scenarios |

## Gaps

- `pi-tui` components are `Rc`-based and the extension component contract is
  `Send + Sync`; every render rebuilds the component tree from declarative
  state. The search `Input` is reconstructed from value + cursor per keystroke
  (added `Input::set_cursor`), so its undo/kill-ring history does not persist
  across keystrokes.
- The component contract exposes no focus state; the search input is rendered
  focused so the cursor is visible.
- `progress()` returns a shared promise in TypeScript; Rust returns a future
  backed by a `Notify` + cancelled flag. A second concurrent waiter is not
  modeled (the source has one caller).

## Discrepancies

- `Math.round`/`toFixed`/`Number::toString` use `pi_js_compat`; the progress bar
  and byte labels match the source for finite inputs.
- `runWithProgress` cancellation has no abort reason payload; cancellation is a
  `CancellationToken` and the run future observes `cancelled()`.

## Proof inventory

- Differential: 4 `runWithProgress` scenarios.
- Behavioral: 6 component cases + TUI command flow (unload path).
