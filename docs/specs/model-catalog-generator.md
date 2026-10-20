# Model catalog generator

Native maintenance port of `packages/ai/scripts/generate-models.ts`. Native JSON catalogs replace TypeScript generated artifacts (approved 2026-10-20); provider values and generator workflows retain source behavior. [Traceability](../porting/traceability/model-catalog-generator.md).

## What it must do

- [x] Preserve models.dev plain costs and context-sized pricing tiers.
- [x] Transform Baseten entries, including deprecated filtering, toggle/effort compatibility and GLM-5.2 overrides.
- [x] Transform Fireworks tool models, including GLM-5p2/Kimi-K3 API and compatibility selection.
- [x] Merge implemented compatibility/thinking/strict-tools/grammar/tool-search/cache metadata in source order without replacing existing user fields.
- [x] Transform every source models.dev provider family in source order for the all-provider fixture, including endpoint routing, model filters, aliases and subscription costs.
- [ ] Fetch, merge, filter, supplement and enrich all source providers with exact priority, ordering and errors.
- [x] Retain raw provider JSON UTF-16 keys/strings without replacement in implemented provider and HTTP transformations.
- [x] Parse source CLI flags and report unknown/conflicting/missing argument errors in source precedence.
- [x] Fetch OpenRouter/AI Gateway through an injected transport, preserving tool filters, pricing, defaults and strict HTTP-status errors.
- [x] Report implemented Baseten/Fireworks/cost malformed raw-input errors instead of silently narrowing them away.
- [ ] Support source CLI flags, native JSON artifacts, checksums/manifests, staged validation and rollback.
- [ ] Default generation updates native provider include-table glue with JSON values; data-only preserves the include table. Runtime catalog APIs remain unchanged.
- [ ] Preserve failure names/messages, log order and exit behavior; omit JS-specific stack frames (approved “Omit JS frames (Recommended)”, 2026-10-20), retaining full raw TS diagnostics in oracle logs.

## How it works

- [Module traceability](../porting/traceability/model-catalog-generator.md).

## Implementation inventory

- `crates/pi-ai/src/model_catalog_generator.rs`: lossless provider transformations, transport and option parsing.
- `crates/pi-ai/src/model_catalog_generator/metadata.rs`: native metadata mutations.
- `crates/pi-ai/src/model_catalog_generator/models_dev.rs`: models.dev/NVIDIA fetching and provider transformations.
- `oracle/model-catalog-generator-oracle.ts`: instrumented pinned source functions with mocked HTTP.

## Tests asserting this spec

- `crates/pi-ai/tests/model_catalog_generator.rs`: original-source cost and provider comparisons.
- `crates/pi-ai/tests/model_catalog_generator_http.rs`: mocked HTTP values and strict/default failures.
- `crates/pi-ai/tests/model_catalog_generator_options.rs`: source flags and validation ordering.
- `crates/pi-ai/tests/model_catalog_generator_raw.rs`: malformed provider errors and UTF-16 identity.
- `crates/pi-ai/tests/model_catalog_generator_metadata.rs`: 675 merged-model cases against original source metadata mutations.
- `crates/pi-ai/tests/model_catalog_generator_models_dev.rs`: all-provider pipeline with independently mocked models.dev/NVIDIA responses.

## Known gaps (current cycle)

- [ ] Remaining fetching/transformation/metadata/CLI/FS orchestration branches listed in traceability.
- [ ] Native model-data helper integration pending parent integration.

## Out of scope

- Live catalog refresh, baseline snapshot changes and deployment.
- Invalid input types excluded by the existing typed reasoning-option API; provider values themselves remain lossless JSON.
