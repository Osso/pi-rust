# image-models traceability

Source: `packages/ai/src/image-models.ts` and `packages/ai/src/image-models.generated.ts`.
Rust: `crates/pi-ai/src/image_models.rs`, `crates/pi-ai/src/image_models_generated.rs`, data `crates/pi-ai/data/image-models.json`.
Tests: `crates/pi-ai/tests/image_models.rs` (5 behavioral), `crates/pi-ai/tests/differential_image_models.rs` (full literal catalog, ordered providers/lists, all 42 union lookups and missing sentinels vs pinned TS). These cover static values only; whole helper-module parity remains **OPEN**.

## Representation
- `IMAGE_MODELS` is embedded as the TypeScript-generated JSON (`data/image-models.json`), parsed once through `LazyLock` into `Vec<(provider, Vec<ImagesModel>)>`. `serde_json`'s `preserve_order` keeps the generated provider/model insertion order; `float_roundtrip` keeps cost values bit-identical (for example `0.19999999999999998`).
- The TypeScript registry is a `Map<string, Map<string, ImagesModel>>`; the Rust catalog keeps the same order without a duplicate registry.
- `getImageModel(provider, modelId)` returns `Option<&'static ImagesModel>` because the TypeScript signature claims a model but returns `undefined` for unknown pairs.
- `getImageProviders()` → `Vec<&'static str>`; `getImageModels(provider)` → `&'static [ImagesModel]` (empty slice for unknown providers).

## Symbols → proof
| TS | Rust | Test |
|---|---|---|
| `IMAGE_MODELS` registry construction | `image_model_groups` | `openrouter_catalog_is_present_with_generated_values`, differential catalog |
| `getImageModel` | `get_image_model` | `model_lookup_returns_generated_model_and_none_for_missing`, differential lookups |
| `getImageProviders` | `get_image_providers` | `providers_are_listed_in_catalog_order`, differential |
| `getImageModels` (unknown provider → `[]`) | `get_image_models` | `unknown_provider_has_no_models`, differential per-provider lists |
| generated values (names, modalities, baseUrl, costs) | JSON data | `model_lookup_returns_generated_model_and_none_for_missing`, differential |

## Data provenance

[Generated-module traceability](image-models-generated.md) owns the frozen payload, exact hashes and bounded full-record/order proof; [static catalog spec](../../specs/image-catalog.md) defines acceptance.

## Gaps
- Whole helper/API parity is OPEN pending user classification of mutable getter-result sharing. Frozen TS public getters return mutable model objects shared across direct lookups, list elements and static provider elements; list containers themselves are fresh. Rust static getters expose immutable references, while provider getters return owned copies. No shared-mutable fields, new API, approved deviation or source-bug classification was introduced.

## Discrepancies
- The concrete mutable-return counterexample is recorded in `/tmp/pi-rust-repin-image-catalog-verify/REPORT.md`; finite generated-data acceptance does not resolve it. Whole provider flow remains OPEN and unchanged by this slice.
