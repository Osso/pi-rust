# Model catalog generator

Port frozen `packages/ai/scripts/generate-models.ts` at4f9e6f8a4. Native JSON/include artifacts retain the2026-10-20 representation approval **to re-validate against4f9e6f8a4**. [Traceability and historical proof retirement](../porting/traceability/model-catalog-generator.md).

## Required behavior

- [ ] Match the four fetchers, every models.dev provider branch, transforms, metadata mutations, overrides/supplements and first-source grouping. Preserve UTF16 strings/keys and source ordering.
- [ ] Match external JSON field uses/defaults/coercions/throws. External HTTP/JSON is not typed-core input. The2026-10-20 custom native receiver/emission TypeError deviation is superseded by2026-10-24 ruling; no schema validation replaces frozen operations. Test consumed malformed fields, not untouched fields.
- [ ] Ignore argv like frozen; fetch sequentially, consume HTTP bodies regardless of status, catch each fetch to its frozen empty result and console arguments. Top-level script throws reach console.error without an added failure exit.
- [ ] Delete old provider JSON before writing any new provider, then write include glue, then print statistics in frozen order. No transactions, rollback, manifest/timestamp validation, selected-subset hydration or alternate public/JSON-only/data-only modes.
- [ ] JSON/include representation must roundtrip quoted/backslash/lone-surrogate model IDs and strings. Frozen unescaped TS interpolation can produce malformed source: record an unrepresentable DISCREPANCY, never fabricate equivalent malformed JSON.

## Implementation

- `crates/pi-ai/src/model_catalog_generator.rs`: injected fetch, native console argument branding, shared CallbackError, HTTP transforms.
- `model_catalog_generator/{models_dev,metadata,catalog,values}.rs`: provider transforms, metadata, source catalog workflow and consumed JSON operations.
- `model_catalog_generator/artifacts.rs`: source-ordered filesystem writes, grouped JSON, include glue.
- `crates/pi-ai/src/bin/generate-models.rs`: explicit maintenance entrypoint; no ordinary runtime refresh.
- `oracle/generator-repin-oracle.mts`: AST exposure of unchanged frozen functions, fixture fetch, filesystem event capture and ordered intermediate catalog capture before emission.

## Proof scope (ongoing)

`generator_repin_{metadata,fetch,catalog,ingress,artifacts}` tests cover metadata combinations, fetch/provider smoke, catalog overrides/supplements, consumed malformed payloads and JSON roundtrip/deletion ordering. The full common-pin branch matrix, real filesystem fault ordering, legacy corpus migration, final diagnostics/clippy/readability and independent gate remain open. Historical tests are not current-pin evidence.

## Out of scope

Live catalog refresh, baseline data mutation, deployment and source runtime/pin changes.
