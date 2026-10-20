# Model catalog generator

Native maintenance port of `packages/ai/scripts/generate-models.ts`. Native JSON catalogs replace TypeScript generated artifacts (approved 2026-10-20); provider values and generator workflows retain source behavior. [Traceability](../porting/traceability/model-catalog-generator.md).

## What it must do

- [x] Preserve models.dev plain costs and context-sized pricing tiers.
- [x] Transform Baseten entries, including deprecated filtering, toggle/effort compatibility and GLM-5.2 overrides.
- [x] Transform Fireworks tool models, including GLM-5p2/Kimi-K3 API and compatibility selection.
- [ ] Fetch, merge, filter, supplement and enrich all source providers with exact priority, ordering and errors.
- [ ] Retain raw provider JSON UTF-16 keys/strings without replacement.
- [ ] Support source CLI flags, native JSON artifacts, checksums/manifests, staged validation and rollback.

## How it works

- [Module traceability](../porting/traceability/model-catalog-generator.md).

## Implementation inventory

- `crates/pi-ai/src/model_catalog_generator.rs`: lossless provider transformations.
- `oracle/model-catalog-generator-oracle.ts`: instrumented pinned source functions with mocked HTTP.

## Tests asserting this spec

- `crates/pi-ai/tests/model_catalog_generator.rs`: original-source cost and provider comparisons.

## Known gaps (current cycle)

- [ ] Remaining fetching/transformation/metadata/CLI/FS orchestration branches listed in traceability.
- [ ] Native model-data helper integration pending parent integration.

## Out of scope

- Live catalog refresh, baseline snapshot changes and deployment.
- Invalid input types excluded by the existing typed reasoning-option API; provider values themselves remain lossless JSON.
