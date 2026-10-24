# Model catalog generator

Port frozen `packages/ai/scripts/generate-models.ts` at4f9e6f8a4. Native JSON/include artifacts retain the2026-10-20 representation approval **to re-validate against4f9e6f8a4**. [Traceability and historical proof retirement](../porting/traceability/model-catalog-generator.md).

## Required behavior

- [x] Match the four fetchers, every models.dev provider branch, transforms, metadata mutations, overrides/supplements and first-source grouping. Preserve UTF16 strings/keys and source ordering.
- [x] Match external JSON field uses/defaults/coercions/throws. External HTTP/JSON is not typed-core input. The2026-10-20 custom native receiver/emission TypeError deviation is superseded by2026-10-24 ruling; no schema validation replaces frozen operations. Test consumed malformed fields, not untouched fields.
- [x] Ignore argv like frozen; fetch sequentially, consume HTTP bodies regardless of status, catch each fetch to its frozen empty result and console arguments. Top-level script throws reach console.error without an added failure exit.
- [x] Delete old provider JSON before writing any new provider, then write include glue, then print statistics in frozen order. No transactions, rollback, manifest/timestamp validation, selected-subset hydration or alternate public/JSON-only/data-only modes.
- [x] JSON/include artifacts store evaluated frozen module values, not intermediate raw records: interpolated strings use shared JS ToString followed by string-literal grammar; numeric literal expressions use shared number parsing. No evaluation of injected expressions. Malformed source, nonfinite values without an artifact encoding, and unsupported expressions fail emission explicitly with provider/model/field DISCREPANCY records.
- [x] Preserve UTF-16 upstream. Only interpolated emitted source crosses shared Node UTF-8 encoding, reproducing frozen lone-unit replacement at writeFileSync; escaped JSON fields and escaped string literals retain their evaluated units. Opaque provider/model/API IDs reject lone units explicitly at generator and lossless reader boundaries.
- [x] Never emit an artifact rejected by the typed Model reader. Wrongly-typed materialized literals fail with provider/model/field context (approved gap, 2026-10-24; same typed-field precedent as the 2026-10-22 catalog-cache approval). Reader invalid-record errors stay explicit.

## Implementation

- `crates/pi-ai/src/model_catalog_generator.rs`: injected fetch, native console argument branding, shared CallbackError, HTTP transforms.
- `model_catalog_generator/{models_dev,metadata,catalog,values}.rs`: provider transforms, metadata, source catalog workflow and consumed JSON operations.
- `model_catalog_generator/artifacts.rs`: source-ordered filesystem writes, grouped JSON, include glue.
- `crates/pi-ai/src/bin/generate-models.rs`: explicit maintenance entrypoint; no ordinary runtime refresh.
- `oracle/generator-repin-oracle.mts`: AST exposure of unchanged frozen functions, fixture fetch, filesystem event capture and ordered intermediate catalog capture before emission.

## Proof scope

`generator_repin_{metadata,fetch,catalog,ingress,emission,filesystem,artifacts}` and migrated surviving fixture corpora cover the whole current-pin module. Proof includes5880 ordered metadata cases, consumed malformed JSON/ID failure ordering, actual written/evaluated TS artifacts, compiled generated include→lossless reader→Model→cost consumer, and real filesystem failure boundaries. Named retirement and exact proof levels live in traceability. Historical old-pin tests are not current-pin evidence; the executable live refresh is intentionally not run.

## Out of scope

Live catalog refresh, baseline data mutation, deployment and source runtime/pin changes.
