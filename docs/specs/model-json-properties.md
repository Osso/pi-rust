# Native model JSON properties

`Model` values from frozen `core/model-catalog-cache.ts` and `core/model-registry.ts` reach RPC unchanged. [Traceability](../porting/traceability/model-json-properties.md) records source consumers and current proof.

## What it must do

- [x] Preserve valid known values, unknown top-level fields, native unknown JSON text and interleaved original object-key order.
- [x] Preserve `Model.name` as native UTF-16 human display text. Reject unpaired units in opaque `Model.id` explicitly.
- [x] Preserve valid nonfinite optional numeric values until output serialization via the [shared native typed decoder](js-json.md#native-typed-decode-2026-10-22), not a per-field projection.
- [x] Reject null optional known top-level fields explicitly instead of silently dropping them (user-approved persisted-input gap, 2026-10-22).
- [x] Preserve nested cost and cost-tier unknown properties/native values and their interleaved key order.
- [ ] Preserve nested unknown properties and nested key order in typed compat objects.
- [ ] Preserve source override-spread/reconstruction ordering and property retention.
- [ ] Preserve complete model JSON in RPC get_available_models/set_model/get_state.

## How it works

- [Native carrier and observable source consumers](../porting/traceability/model-json-properties.md).

## Implementation inventory

- `crates/pi-ai/src/types.rs`: typed model values and native name, explicit typed-boundary rejection.
- `crates/pi-ai/src/types/model_properties.rs`: unknown-only properties and original order metadata; no duplicate known-value store.
- `crates/pi-coding-agent-interactive/src/modes/interactive/model_search.rs`: native display-name search candidates.

## Tests asserting this spec

- `crates/pi-coding-agent/tests/differential_model_catalog_cache.rs`: actual frozen cached-model JSON order, raw name/unknown text and explicit opaque/null rejection.

## Known gaps (current cycle)

- [ ] Complete compat property carriers and registry/RPC integration proof.
- [ ] Full constructor/test-fixture migration and all-target checkpoint.

## Out of scope

Runtime-generated JS stacks are omitted by explicit user approval. Wrongly-typed optional persisted fields reject explicitly; no replacement or silent omission.
