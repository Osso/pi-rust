# image-models.generated traceability

Source: `packages/ai/src/image-models.generated.ts` at `4f9e6f8a4c0065d81b10e7177b28d5082f0bc30d` ([source lock](../source-lock.json)).
Rust: `crates/pi-ai/data/image-models.json`, unchanged `crates/pi-ai/src/image_models_generated.rs`.
Contract: [static generated image catalog](../../specs/image-catalog.md).

## Export / branches → proof

| Exact scope | Proof | Status |
|---|---|---|
| Whole finite `IMAGE_MODELS` literal, all 35 full records, optional absence, arrays and f64 costs | Fresh sandbox literal export; `differential_image_models::image_catalog_matches_the_pinned_typescript_module` | Targeted GREEN at d8fefff1 |
| Provider/model membership and insertion order | Explicit ordered key vectors and complete ordered value arrays in differential | Targeted GREEN at d8fefff1 |
| All 35 source and seven removed native-only IDs (42 union lookups) | Actual frozen getter and native getter each compared with complete source literal records/null | Targeted GREEN at d8fefff1 |
| Missing model/provider and empty missing-provider list | Differential sentinels + `image_models` behavioral tests | Targeted GREEN at d8fefff1 |
| Static provider catalog consumption only | Full native factory list vs source literal; fresh external source factory list also equals literal | Targeted GREEN at d8fefff1 |

## Provenance

Verified 2026-10-21. Fresh process directly imports actual frozen `IMAGE_MODELS`, with no singleton mutation, network generator, fallback or supplementation. Run via `python3 scripts/test-sandbox.py -- <frozen>/node_modules/.bin/tsx --tsconfig <frozen>/tsconfig.json <export.ts>` with a private writable `PI_TEST_SANDBOX_DIR` and `PI_OFFLINE=1`. Frozen files match their exact pinned Git blobs.

- Generated source Git blob: `573386613b6624a65bd38ecb1c914f68ecee875c`.
- Generated source SHA256: `255db32f84f1a94f579f061506ced014ccf33d0b719b4a855eace464cb395a00`.
- Ordered payload SHA256 (`JSON.stringify(IMAGE_MODELS, null, 2)` plus newline): `55746b4d4f679705adb1540448101450ed52ddc85b1ea1e9faa49970e7b51bfd`.
- Complete fresh source payload, ordered key vectors, full native proof output and 42 lookups: `/tmp/pi-rust-repin-image-catalog/{source-catalog.json,source-static.json,provenance.json,REPORT.md,ledger.md}`. These are local proof artifacts, not build inputs.

Compared with b85d01795f614ca85893371c0aef2081c4b43f23: remove seven source-absent records; restore `google/gemini-2.5-flash-image.cost.cacheWrite` to `0.08333333333333334` (bits `0x3fb5555555555556`) and `x-ai/grok-imagine-image-quality.name` to `xAI: Grok Imagine Image Quality`. Remaining full records match, not just counts. Existing common relative order was already correct; `serde_json` `preserve_order`/`float_roundtrip` and loader remain unchanged.

## Development evidence

Existing exact b85 differential RED reused only after byte-for-byte scope validation (`red-reuse-scope.json`); not a fresh RED claim. At implementation commit `d8fefff1e3d4a6783a3a3cba5aa4c5236d74f56c`, focused catalog tests pass (1 differential, 5 behavioral), plus two selected static provider/collection tests. Real embedded-data mutants (old rounded cacheWrite, reintroduced Krea record, swapped first model positions) each fail the differential at the intended semantic assertion. Original payload restored and focused catalog GREEN rebuilt; no mutant committed. Full logs, mutation hashes and command/revision validity are in the local report ledger. Rust fmt passes; no broad suite/check/clippy/readability/final gate was run for this bounded development slice.

## Limits / open semantics

Finite static generated-data proof only. [Helper module](image-models.md) whole parity remains OPEN: typed TS getters share mutable model objects while Rust exposes immutable references and provider-owned copies. User classified this (2026-10-21) as a TS bug fixed by a separate copy-on-read post-pin delta; delta proof pending. Whole provider flow, auth metadata, generation, refresh, arbitrary generator inputs, and source mutation behavior are outside this acceptance. The [maintenance generator](image-catalog-generator.md) does not supply this re-pin.
