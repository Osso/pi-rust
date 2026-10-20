# Model catalog generator

Native maintenance port of `packages/ai/scripts/generate-models.ts`. Native JSON catalogs replace TypeScript generated artifacts (approved 2026-10-20); provider values and generator workflows retain source behavior. [Traceability](../porting/traceability/model-catalog-generator.md).

## What it must do

- [x] Reject wrong copied Model field types at actual artifact emission, including JSON-only, outside fetch catches and after existing staged validators; validate only emitted data-only subsets. No added finite/range/domain limits.
- [x] Reject malformed consumed models.dev provider/model/cost/tier/modalities/reasoning receivers at their use points with native TypeErrors; preserve filters, defaults, unknown strings, unused controls and deferred outer reasoning failures. Remaining response/metadata branches stay partial (approved “Reject wrong field types (Recommended)”, 2026-10-20).
- [x] Consume shared native HTTP error name/message/cause without string parsing; preserve top-level fetch-failed/body-terminated identity and fetch strict/default catch behavior. Native cause diagnostics remain sanitized (approved “Allow native causes (Recommended)”, 2026-10-20).

- [x] Preserve models.dev plain costs and context-sized pricing tiers.
- [x] Transform Baseten entries, including deprecated filtering, toggle/effort compatibility and GLM-5.2 overrides.
- [x] Transform Fireworks tool models, including GLM-5p2/Kimi-K3 API and compatibility selection.
- [x] Merge implemented compatibility/thinking/strict-tools/grammar/tool-search/cache metadata in source order without replacing existing user fields; preserve raw cache comparison coercions and JS ASCII model-family regexes.
- [x] Reject wrong/missing/null generated IDs with a stable native TypeError at the original outer generation use point; do not move the error into default fetch catches. Retain all valid UTF-16 strings and source fetch/log order.
- [x] Transform every source models.dev provider family in source order for the all-provider fixture, including endpoint routing, model filters, aliases and subscription costs.
- [x] Merge fetched arrays, apply source overrides/supplements, clone Codex/GC and Azure, consume reasoning records, deduplicate and sort complete catalog values against full source JSON-only fixtures.
- [ ] Cover all remaining provider/error/collision branches and native artifact workflows.
- [x] Retain raw provider JSON UTF-16 keys/strings without replacement in implemented provider and HTTP transformations.
- [x] Parse source CLI flags and report unknown/conflicting/missing argument errors in source precedence.
- [x] Fetch OpenRouter/AI Gateway through an injected transport, preserving tool filters, pricing, defaults, malformed payload/coercion errors, raw overflow and strict HTTP-status errors.
- [x] Preserve earlier models.dev reasoning records when a later raw provider model throws; discard returned models at the source failure boundary, not partial state.
- [x] Report implemented Baseten/Fireworks/cost malformed raw-input errors instead of silently narrowing them away.
- [x] Preserve internal transform scalar-effort/array-map/pricing semantics and distinct NVIDIA normalized lone-surrogate aliases; malformed response acceptance is superseded by the approved validation boundary.
- [x] Match pinned NVIDIA normalization for every UTF-16 unit, every Unicode scalar and contextual/lone-surrogate fixtures.
- [x] Generate grouped private and flat public JSON, source-identical manifests/checksums/timestamps and pretty/compact bytes in default/public/json-only/data-only fixtures.
- [x] Execute original automatic CLI catch and native maintenance option handling, immediate logs/statistics, strict failure/exit order with fixture HTTP; reject invalid CLI mode in a real subprocess before fetching.
- [x] Preserve four fetchers' strict/default HTTP-status, transport and JSON-parse error identity/log order without retries; preserve NVIDIA Map overwrite order and strict Qwen individual filtering/exact IDs.
- [x] Preserve staged write/validation failure, include-write failure, failed data rename, post-swap manifest/provider/include validation, rollback and cleanup-error behavior against actual source, including filesystem metadata.
- [ ] Prove remaining data-only/public-output guards and native transport identity.
- [x] Default generation updates native provider include-table glue with JSON values; data-only preserves the include table. Runtime catalog APIs and every current embedded value remain unchanged.
- [ ] Preserve failure names/messages, log order and exit behavior; omit JS-specific stack frames (approved “Omit JS frames (Recommended)”, 2026-10-20), retaining full raw TS diagnostics in oracle logs.

## How it works

- [Module traceability](../porting/traceability/model-catalog-generator.md).

## Implementation inventory

- `crates/pi-ai/src/model_catalog_generator.rs`: lossless provider transformations, injected transport, option parsing, CLI orchestration and error records.
- `crates/pi-ai/src/bin/generate-models.rs`: explicit maintenance binary using compiled source root and native HTTP transport.
- `crates/pi-ai/src/model_catalog_generator/metadata.rs`: native metadata mutations.
- `crates/pi-ai/src/model_catalog_generator/models_dev.rs`: models.dev/NVIDIA fetching and provider transformations.
- `crates/pi-ai/src/model_catalog_generator/catalog.rs`: complete fetched-value merge, overrides, supplements, clones and sorted public catalog.
- `crates/pi-ai/src/model_catalog_generator/artifacts.rs`: native JSON/include staging, manifest validation and public output.
- `crates/pi-ai/src/model_catalog_generator/emission_types.rs`: minimal actual-Model field-type checks at emission, after source filters/defaults.
- `crates/pi-ai/src/model_catalog_generator/receiver_types.rs`: consumed models.dev receiver checks, preserving source short-circuit order.
- `crates/pi-ai/src/providers/model_catalogs_generated.rs`: extracted provider include table consumed by unchanged `models_data` APIs.
- `oracle/model-catalog-generator-oracle.ts`: instrumented pinned source functions with mocked HTTP.

## Tests asserting this spec

- `crates/pi-ai/tests/model_catalog_generator.rs`: original-source cost and provider comparisons.
- `crates/pi-ai/tests/model_catalog_generator_http.rs`: mocked HTTP values and strict/default failures.
- `crates/pi-ai/tests/model_catalog_generator_fetch.rs`: all-fetcher error matrix, NVIDIA Map order, strict Qwen set checks and original automatic CLI catch.
- `crates/pi-ai/tests/model_catalog_generator_native_http.rs`: actual refused connections/premature bodies against Node native fetch and shared sanitized causes through generator catches.
- `crates/pi-ai/tests/model_catalog_generator_options.rs`: source flags and validation ordering.
- `crates/pi-ai/tests/model_catalog_generator_raw.rs`: malformed provider errors and UTF-16 identity.
- `crates/pi-ai/tests/model_catalog_generator_receivers.rs`: approved native receiver errors, source short-circuits and deferred reasoning failure order.
- `crates/pi-ai/tests/model_catalog_generator_metadata.rs`: 675 merged-model cases against original source metadata mutations.
- `crates/pi-ai/tests/model_catalog_generator_models_dev.rs`: all-provider pipeline with independently mocked models.dev/NVIDIA responses.
- `crates/pi-ai/tests/model_catalog_generator_workflow.rs`: full source module workflow with fixture HTTP and sandboxed temporary filesystem.
- `crates/pi-ai/tests/model_catalog_generator_artifacts.rs`: source/native artifact bytes, manifests, include-table preservation, compiled fixture inclusion and current embedded values.

## Known gaps (current cycle)

- [ ] Remaining fetching/transformation/metadata/CLI/FS orchestration branches listed in traceability.
- [ ] Native model-data helper integration pending parent integration.

## Out of scope

- Live catalog refresh, baseline snapshot changes and deployment.
- Invalid input types excluded by the existing typed reasoning-option API; provider values themselves remain lossless JSON.
