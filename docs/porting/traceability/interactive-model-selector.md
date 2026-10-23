# model-selector.ts

Authority: common-pin331-line `coding-agent/src/modes/interactive/components/model-selector.ts`. Rust: `crates/pi-coding-agent/src/modes/interactive/components/model_selector.rs`. [Contract](../../specs/model-selector.md).

| Frozen surface | Native representation / behavior | Proof |
|---|---|---|
| constructor :58–142 | real inherited Container with typed Input/list/text handles; actual ModelRegistry.refresh/get_error/get_available execute synchronously, publication/requestRender deferred | nine actual frozen-class/native-registry scenarios in `differential_model_selector`; initial snapshots have no model rows/render request |
| loadModels :145–184 | available snapshot, current-first stable provider sort, scoped find refresh or retained original, min-clamped current/index selection | file mutated after registry construction, before selector construction; refreshed scoped name, malformed models.json, no configured models |
| getAvailable catch :157–169 | Vertex ADC homedir → get_env_api_key → has_auth → has_configured_auth → get_available propagate NativeNodeError as Result; selector consumes the error after deferred publication | `differential_model_selector_nss`: forced getpwuid_r EIO in isolated source/native children, exact one-render error row |
| sortModels :187–199 | current-first, provider localeCompare, stable source order within provider |14 refreshed models and current zeta model before alpha |
| scope/filter :202–239 | source initial scope, retained search child, current reset on scope change, min-clamp (not reset) when filtering | all/scoped/tab, multi-character query retains index2, empty/no-match/raw-surrogate query |
| updateList :242–297 | ten-row window/scroll count, source selected/checkmark/provider/name/error/no-results rows | >10 models; narrow/zero widths; raw refreshed human model.name; scoped rows retained on load error |
| handleInput/select :300–326 | shared keybindings, empty-list guards, wrapping arrows, callback only; cancel callback only | source/native arrows/tab/confirm/cancel, direct actual Input.submit, no results/no callback |
| getSearchInput/focus | live Rc Input; shared filtered/index state feeds onSubmit without reborrowing parent | direct setter does not silently filter; direct submit reads selected model; raw units/focus retained |
| inherited Container | add/remove/clear, render and invalidation operate on mounted children | extra child removal, clear before deferred publication, detached live Input still submits after clear |

## Removed pre-repin contracts

Deleted old-only SettingsManager/theme constructor parameters, refresh-status row/fields, default-model persistence in handleSelect, dispose calls, separate mutable-input getter, hand-assembled render/invalidate paths and reset-to-zero filtering. Source has none. `tests/model_selector.rs` was replaced by the actual frozen class differential: its valid sort/scope/confirm/cancel/no-match/wrap/raw contracts remain covered; old save-default/reset-selection assertions contradicted frozen behavior.

## Source ordering discrepancy retained

Input typed before deferred publication is not automatically re-filtered when initialSearchInput was absent: rows populate with the full available snapshot even though search text already changed. When initialSearchInput was present, the constructor continuation filters that captured initial string, not later input text. Both cases compare actual source snapshots; no guard or helpful repair is added.

## Proof ledger

Artifacts `/tmp/pi-rust-repin-lossless-interactive/`: `model-selector-red.log` reproduces immediate population, old refresh row and missing inherited tree; `model-selector-green.log` passes all nine scenarios. `model-selector-nss-source.log` proves actual frozen ModelRegistry's fallible ADC path: one render with `A system error occurred: uv_os_homedir returned EIO (i/o error)`. `nss-eio.c/.so` and `model-selector-nss-probe.mts` reproduce in a child only; no live credentials/network involved.

Vertex catch RED/GREEN: `model-selector-nss-red.log` / `model-selector-nss-green.log`; RED rendered “No matching models”, GREEN matches frozen EIO rows exactly. `vertex-model-tests.log`: selector class differential, list-models differential and 41 resolver tests GREEN. Parent owns final independent/workspace gates.
