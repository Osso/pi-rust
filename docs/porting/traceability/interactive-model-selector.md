# model-selector.ts

Source: `coding-agent/src/modes/interactive/components/model-selector.ts`
Rust: `crates/pi-coding-agent/src/modes/interactive/components/model_selector.rs`

| Symbol / branch | Rust item | Proof |
|---|---|---|
| `ModelItem` / `ScopedModelItem` | `ModelItem`, `ScopedModel` (`model_resolver`) | `model_selector.rs` |
| `ModelScope` | `ModelScope` | tab scope test |
| title/scope hint/warning | `new` + `get_scope_text` / `get_scope_hint_text` | scope + warning assertions |
| search input + initial value | `search_input` | search test |
| `loadModelsFromSnapshot` | `load_models_from_snapshot` | sort/render tests |
| `refreshModels` (15s timeout, abort, error messages) | `refresh_models` (async) | gap: needs mocked network refresh |
| `dispose` | `dispose` | cancel test (dispose then callback) |
| `sortModels` current-first then provider | `sort_models` (`models_are_equal`, `locale_compare`) | sort test |
| `setScope` | `set_scope` | tab test |
| `filterModels` reset-to-top on query | `filter_models` | search test (selected index 1 → 0) |
| 10-row window + scroll info | `update_list` | gap: needs >10 models |
| selected/unselected rows, current checkmark | `update_list` | sort test |
| error lines / no matching / model name / refresh status | `update_list` | no-matching test; error path gap |
| up/down wrap | `handle_input` | wrap test |
| confirm → save default + callback | `handle_select` | confirm test |
| cancel → dispose + callback | `handle_input` | cancel test |
| search fallthrough | `handle_input` | search test |
| focus propagation | `Focusable` | code-mapped |

Adaptations: `TUI` becomes `ModelSelectorUi::request_render`; the constructor's fire-and-forget `void refreshModels()` becomes an explicit `async fn refresh_models` for the caller to spawn (no hidden runtime); `SettingsManager` is `Rc<RefCell<..>>`.

Discrepancy: `ModelsRefreshResult.errors` is a `BTreeMap`, so the "Could not refresh N model catalogs (...)" provider list is sorted, while the TypeScript `Map` preserves insertion order. The TS refresh test (openai, anthropic order) therefore has no Rust port; the refresh path needs a mockable runtime to test.

Gap: the TS refresh error-list test depends on `vi.spyOn(modelRuntime, "refresh")`; the Rust component takes the concrete `ModelRuntime`, so that test has no equivalent.

## Q18 native UTF-16

Search input/query, refresh/error text and render output retain `JsString`. Runtime errors use their raw `message()` authority, not Display. Native query filtering against approved scalar model metadata. `model_selector::native_search_retains_unmatched_units_without_selecting_a_model` written **unexecuted**, library blocked.
