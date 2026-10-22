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

Pending common-pin reconciliation: frozen config :74–77 has no refresh status; old Rust refresh row/API and interactive-mode callers still require removal. This is not an approved deviation. Exact differential remains RED until that removal lands.
