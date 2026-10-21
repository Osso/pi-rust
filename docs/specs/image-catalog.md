# Static generated image catalog

The bounded catalog contract is the complete `IMAGE_MODELS` literal exported by `packages/ai/src/image-models.generated.ts` at the [common source pin](../porting/source-lock.json). [Traceability and provenance](../porting/traceability/image-models-generated.md) distinguish this finite data proof from mutable helper/provider API parity.

## What it must do

- [ ] Preserve every record and field (including optional-field absence and exact floating-point values), with no supplements or source-absent records.
- [ ] Preserve provider/model insertion order and membership through static lists and lookups.
- [ ] Return full matching records for all source IDs and absence for all seven removed native-only IDs; missing provider/model sentinels return absence/empty lists.
- [ ] Static OpenRouter provider catalog consumption preserves all full records in source order, without invoking auth, refresh or generation.

## How it works

- [Generated catalog traceability](../porting/traceability/image-models-generated.md).
- [Existing static helper representation and open semantics](../porting/traceability/image-models.md).

## Implementation inventory

- `crates/pi-ai/data/image-models.json`: whole serialized frozen literal.
- `crates/pi-ai/src/image_models_generated.rs`: unchanged ordered embedded-data loader.
- `crates/pi-ai/src/image_models.rs`: unchanged static lookup/list helpers.
- `crates/pi-ai/src/providers/openrouter_images.rs`: unchanged static catalog consumer; whole provider parity is not claimed.

## Tests asserting this spec

- `crates/pi-ai/tests/differential_image_models.rs`: actual frozen literal vs full native values; explicit ordered keys/lists; every source plus removed-ID lookup; full static provider records.
- `crates/pi-ai/tests/image_models.rs`: concrete names, modalities, prices/f64 bits, removed IDs and absent branches.
- `crates/pi-ai/tests/openrouter_images_provider.rs`: full static provider/collection catalog equality instead of old count/first-record checks.

## Known gaps (current cycle)

- [ ] Mutable getter-result sharing vs immutable Rust references/owned provider copies awaits user classification; no waiver or shared-mutable redesign implied.
- [ ] Whole `image-models.ts` and provider-flow parity remains OPEN; unchanged by finite generated-data acceptance.

## Out of scope

- Runtime provider/auth/refresh/network behavior, arbitrary catalog generation inputs, helper mutation semantics, invalid JavaScript inputs, chat catalogs and model/API redesign: separate contracts, not generated-literal data.
