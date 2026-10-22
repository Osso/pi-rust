# scoped-models-selector.ts

Frozen source: `4f9e6f8a4`, `coding-agent/src/modes/interactive/components/scoped-models-selector.ts`.
Rust: `crates/pi-coding-agent/src/modes/interactive/components/scoped_models_selector.rs`.

| Branch | Native behavior | Proof |
|---|---|---|
| `buildItems` :154–162 filters missing model IDs | only existing models become items; model field is not optional | `raw_error_consumer_components::scoped_models_filters_raw_missing_ids_and_preserves_search_input`; scoped-subset differential |
| footer :165–179 | enabled count uses configured ID-list length, including missing IDs; no unavailable-count suffix | exact scoped-model ANSI differential |
| search :183–191 | native query against existing model candidates | raw missing-ID/search test; scoped-search differential |
| selection/reorder/provider toggle | native full IDs and callbacks | `differential_components::scoped_models_selector_matches_typescript` |

2026-10-22 RED: old Rust retained missing IDs and rendered `[unavailable]`; frozen filters them out. Raw regression also failed because `No matching models` was absent (`test-scoped-filter-red.log`). Missing raw IDs now remain configuration data but cannot become selectable rows; native search input still preserves their units.

## Old-only refresh removal (2026-10-22)

Frozen config :74–77 has no refresh status; removed native refresh-status row, field, setter, enum and `update_models`. Frozen `interactive-mode.ts::showModelsSelector` :6154–6186 refreshes/reads available models before constructing the selector; after mounting it only keeps change/persist/cancel callbacks. Removed the native post-mount catalog request, timeout task, mutable catalog cells, disposal cancellation and status rendering, plus its now-unused shared-component adapter. No background request or error result is left silently discarded.

Pre-mount reload/get-available and their native error paths remain. Current enabled IDs follow frozen :6188–6200: session scope first, otherwise resolved configured patterns, without appending unresolved patterns. Change callback follows :6203–6220: only a nonempty list shorter than available count sets scope; all/none clears it. Persist and cancel callbacks remain.

The existing scoped-subset exact-frame differential is the refresh-row RED. Whole interactive mode has no TS differential (approved project gap); removal of post-mount asynchronous work is grounded in the complete frozen method above, not claimed as a process-lifetime differential proof.
