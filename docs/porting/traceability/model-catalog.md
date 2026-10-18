# model-catalog traceability

Source: `packages/ai/src/model-catalog.ts`.
Rust: `crates/pi-ai/src/model_catalog.rs`.
Tests: `crates/pi-ai/tests/model_catalog.rs` (3 behavioral), `crates/pi-ai/tests/differential_model_catalog.rs` (4 requests vs pinned TS).

## Representation
- `ModelGroups = Record<string, Record<string, object>>` → `BTreeMap<String, BTreeMap<String, Model>>`.
- `ModelCatalog` → `BTreeMap<String, Model>`.
- `flattenModelCatalog(provider, groups)` returns `Object.assign({}, ...Object.values(groups))`; the `provider` parameter is type-only, and model IDs/providers are already present in each model value. The Rust function therefore takes only the groups.
- `Object.assign` preserves first-insertion order and lets later groups overwrite duplicate keys; `BTreeMap::extend` keeps the last value and sorts keys (semantic JSON equal).

## Symbols → proof
| TS | Rust | Test |
|---|---|---|
| `flattenModelCatalog` | `flatten_model_catalog` | `flattens_groups_and_keeps_the_last_value_for_duplicate_ids`, `empty_catalog_flattens_to_an_empty_map`, differential |
| `ModelCatalog` type | `ModelCatalog` alias | `model_cost_tiers_serialize_like_generated_data` |
| `ModelGroups` type | `ModelGroups` alias | compile-time; exercised by tests |

## Gaps
- The TypeScript generic constraints (`ModelGroups`, `ModelId`, `ModelApi`) are compile-time only; Rust uses concrete `Model` values. Invalid generic inputs cannot be expressed.

## Discrepancies
- None.
