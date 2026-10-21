# providers/all traceability

Current literal catalog proof: [37-module boundary](providers-models.md). The three direct catalog lookups plus the separately authorized factory ID/order inventory are covered at the common pin; factory/whole-barrel rows below retain historical proof and are not promoted.

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
| source-absent data URL | deleted | no production consumers |
| source-absent timestamp | retained staged model_runtime dependency only | no frozen-source parity; see [catalog boundary](providers-models.md) |
| `builtinProviders()` ID/order only | `builtin_providers()` inventory | new `differential_provider_inventory` actual-source RED (old baseten panic) → GREEN; order mutant killed; factory/auth/protocol whole parity remains TODO |
| `builtinModels(options?)` | `builtin_models(options)` | `builtin_models_registers_every_builtin_provider_with_models` |
| `builtinImagesProviders()` | `builtin_images_providers()` | `builtin_images_providers_and_models_register_openrouter` |
| `builtinImagesModels(options?)` | `builtin_images_models(options)` | same |

## Branch notes

- Artifact-deletion coupling re-pins only builtin provider ID/order inventory: 36 source entries including default/GC/Team Codex; removes baseten/qwen/radius entries. Existing generic Codex factory handles Team without a protocol/auth rewrite.
- Whole builtinModels/factory behavior and mutable catalog alias contracts retain TODO coverage; historical tests above do not establish new-pin parity.

## Staged APIs

Source-absent data-URL helper deleted. Timestamp helper retained only for the coding-agent model_runtime consumer; returns None without invented source metadata.

## Discrepancies

- None.
