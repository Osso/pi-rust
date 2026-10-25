# Native model JSON properties

`Model` values from frozen `core/model-catalog-cache.ts` and `core/model-registry.ts` reach RPC unchanged. [Traceability](../porting/traceability/model-json-properties.md) records source consumers and current proof.

## What it must do

- [x] Preserve valid known values, unknown top-level fields, native unknown JSON text and interleaved original object-key order.
- [x] Preserve `Model.name` as native UTF-16 human display text. Reject unpaired units in opaque `Model.id` explicitly.
- [x] Preserve valid nonfinite optional numeric values until output serialization via the [shared native typed decoder](js-json.md#native-typed-decode-2026-10-22), not a per-field projection.
- [x] Reject null optional known top-level fields explicitly instead of silently dropping them (user-approved persisted-input gap, 2026-10-22).
- [x] Preserve nested cost and cost-tier unknown properties/native values and their interleaved key order.
- [x] Preserve nested unknown properties and nested key order in typed compat objects.
- [ ] Preserve source override-spread/reconstruction ordering and property retention.
- [ ] Preserve complete model JSON in RPC get_available_models/set_model/get_state.

## How it works

- [Native carrier and observable source consumers](../porting/traceability/model-json-properties.md).

## Compat representation (2026-10-24)

Compat records store each frozen-known value once as Option<JsValue>; ModelProperties stores only unknown keys and original order. Missing and present-null remain distinct. Nested routing/kwargs values stay lossless JSON, including unknown fields and UTF16 units. Frozen-absent compat fields are ordinary unknown keys, not typed behavior switches. Native consumer accessors implement the exact frozen nullish-default, truthy, not-false, strict-true and strict-string-equality read; no speculative ToString operation is added for maxTokensField. Catalog known-type validation remains separate from schema-validated models.json ingress and runs before user overrides.

## Thinking-level maps (2026-10-24)

Known thinking-level values retain exact UTF16 strings or explicit null. Missing keys remain missing. Preserve schema-accepted unknown keys/JSON and the original interleaved order, including through provider/model merges and serialized Model/ImageModel output. Known wrong types keep the frozen models.json schema rejection. Provider reads retain their literal frozen nullish/string/truthiness operations; selected values must reach JSON request bodies without scalar projection. An approved opaque header/ID boundary still rejects unpaired units explicitly if one is reached.

## Implementation inventory

- `crates/pi-ai/src/types.rs`: typed model values and native name, explicit typed-boundary rejection.
- `crates/pi-ai/src/types/compat.rs`: single-authority raw compat fields, order/unknown carrier and frozen semantic accessors. Completions/Responses/Anthropic consumers use literal source reads; source-absent fields have no adapter behavior.
- `crates/pi-ai/src/types/model_properties.rs`: unknown-only properties and original order metadata; no duplicate known-value store.
- `crates/pi-ai/src/types/thinking_level_map.rs`: known nullable JsString values plus disjoint unknown/order metadata; exact selected-value requests and complete configuration/order snapshots pass.
- `crates/pi-coding-agent-ui/src/modes/interactive/model_search.rs`: native display-name search candidates.

## Tests asserting this spec

- `crates/pi-coding-agent/tests/differential_model_catalog_cache.rs`: actual frozen cached-model JSON order, raw name/unknown text and explicit opaque/null rejection.
- `crates/pi-coding-agent/tests/model_registry_repin.rs`: complete actual-source configuration models/errors/serialized order, including raw compat/null/unknown fields and nested UTF16 spread.
- `crates/pi-coding-agent/tests/model_registry_compat_requests.rs`: actual-source models.json requests for16 removed fields and literal read controls, including Bedrock's real loopback SDK request.

## Known gaps (current cycle)

- [ ] Complete compat property carriers and registry/RPC integration proof.
- [x] Full constructor/test-fixture migration and all-target checkpoint for compat/thinking-map carriers; broader model/RPC closure remains above.

## Out of scope

Runtime-generated JS stacks are omitted by explicit user approval. Wrongly-typed optional persisted fields reject explicitly; no replacement or silent omission.
