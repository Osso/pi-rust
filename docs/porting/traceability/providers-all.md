# providers/all traceability

Pinned source: `packages/ai/src/providers/all.ts` at
`532f7d12cd19235081bd4d6e425bde185652d86f`.
Rust: `crates/pi-ai/src/providers/all.rs`.
Tests: `crates/pi-ai/tests/providers.rs`,
`crates/pi-ai/tests/differential_providers.rs`.

| TypeScript | Rust | Proof |
|---|---|---|
| `getBuiltinModel(provider, modelId)` | `get_builtin_model` | `catalog_keeps_ts_generated_order_and_metadata`, differential catalog |
| `getBuiltinProviders()` | `get_builtin_providers()` | `builtin_provider_order_matches_all_ts_without_codex`, differential catalog |
| `getBuiltinModels(provider)` | `get_builtin_models` | differential catalog (all 40 providers) |
| `getBuiltinModelDataUrl(provider)` | `get_builtin_model_data_url` | `catalog_helpers_report_manifest_metadata` |
| `getBuiltinModelDataGeneratedAt()` | `get_builtin_model_data_generated_at` | `catalog_helpers_report_manifest_metadata`, differential catalog |
| `builtinProviders()` | `builtin_providers()` | `builtin_provider_order_matches_all_ts_without_codex`, `provider_metadata_matches_the_ts_factories`, differential provider descriptors |
| `builtinModels(options?)` | `builtin_models(options)` | `builtin_models_registers_every_builtin_provider_with_models` |
| `builtinImagesProviders()` | `builtin_images_providers()` | `builtin_images_providers_and_models_register_openrouter` |
| `builtinImagesModels(options?)` | `builtin_images_models(options)` | same |

## Branch notes

- `builtinProviders()` constructs fresh instances in the TypeScript array order;
  the Rust vec keeps that order minus the two codex entries.
- `builtinModels` registers every provider on a fresh `Models` collection;
  Radius registers with an empty catalog (dynamic only).

## Gaps

- `openaiCodexProvider()` and `openaiCodexProvider("openai-codex-gc", ...)` are
  not ported here (api-codex slice). The differential asserts the TS list minus
  Rust is exactly `["openai-codex", "openai-codex-gc"]`.
- `getBuiltinModelDataUrl` returns the `file://` URL of the embedded
  `data/providers/<provider>.json`; Rust has no generated per-provider module.
  The helper is dev-only and untested in TypeScript.

## Discrepancies

- None.
