# image-models traceability

Source: `packages/ai/src/image-models.ts` and `packages/ai/src/image-models.generated.ts`.
Rust: `crates/pi-ai/src/image_models.rs`, `crates/pi-ai/src/image_models_generated.rs`, data `crates/pi-ai/data/image-models.json`.
Tests: `crates/pi-ai/tests/image_models.rs` (5 behavioral), `crates/pi-ai/tests/differential_image_models.rs` (full catalog, provider list, per-provider lists, and five lookups vs pinned TS).

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
`data/image-models.json` is `JSON.stringify(IMAGE_MODELS, null, 2)` from the pinned source worktree:

```sh
"$SOURCE/node_modules/.bin/tsx" --tsconfig "$SOURCE/tsconfig.json" -e \
  'import { IMAGE_MODELS } from "'"$SOURCE"'/packages/ai/src/image-models.generated.ts";
   console.log(JSON.stringify(IMAGE_MODELS, null, 2));'
```

The catalog contains 42 OpenRouter image models at the pinned revision. This baseline remains unchanged. The separate [image catalog maintenance generator](image-catalog-generator.md) targets this native artifact; its implementation/tests do not refresh the checked-in source of truth.

## Gaps
- None.

## Discrepancies
- None.
