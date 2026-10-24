# providers/all traceability

Current literal catalog proof: [37-module boundary](providers-models.md). Current descriptor/auth/dispatch scope and integration checks: [Q15 checkpoint](../q15-sdk-provider-proof.md). Neither establishes whole-barrel parity.

Current source: `packages/ai/src/providers/all.ts` at [the common pin](../source-lock.json).
Rows not explicitly re-proven below retain historical `532f7d12cd19235081bd4d6e425bde185652d86f` evidence only.
Rust: `crates/pi-ai/src/providers/all.rs`.
Tests: `crates/pi-ai/tests/providers.rs`,
`crates/pi-ai/tests/differential_providers.rs`.

| TypeScript | Rust | Proof |
|---|---|---|
| `getBuiltinModel(provider, modelId)` value lookup only | `get_builtin_model` | `differential_chat_catalog`: every literal lookup, unknown IDs; mutable alias/reference contract TODO |
| `getBuiltinProviders()` ordered data keys only | `get_builtin_providers()` | `differential_chat_catalog`: all 36 ordered source keys |
| `getBuiltinModels(provider)` ordered values only | `get_builtin_models` | `differential_chat_catalog`: all 1058 values/order; mutable alias/reference contract TODO |
| source-absent data URL | deleted | no production consumers |
| source-absent timestamp | deleted after model_runtime removal | consumer inventory had only two legacy tests; current catalog differential invokes real source exports |
| `builtinProviders()` descriptor/order/picker IDs | `builtin_providers()` | current differential_providers GREEN6 includes all36 descriptors, full auth/login and fetch-dispatch records; [limits](../q15-sdk-provider-proof.md) |
| `builtinModels(options?)` | `builtin_models(options)` | `builtin_models_registers_every_builtin_provider_with_models` |
| `builtinImagesProviders()` | `builtin_images_providers()` | `builtin_images_providers_and_models_register_openrouter` |
| `builtinImagesModels(options?)` | `builtin_images_models(options)` | same |

## Branch notes

- Artifact-deletion coupling re-pins only builtin provider ID/order inventory: 36 source entries including default/GC/Team Codex; removes baseten/qwen/radius entries. Existing generic Codex factory handles Team without a protocol/auth rewrite.
- Whole builtinModels/factory behavior and mutable catalog alias contracts retain TODO coverage; historical tests above do not establish new-pin parity.

## Current-pin six-case revalidation (2026-10-23)

At b565d620, all six differential_providers tests fail. Stale oracle/fixture inputs are classified against frozen source: all.ts:79–117 removes radius; all.ts has no generation-timestamp export; models.ts:216–219 requires a model, not a provider ID; amazon-bedrock.ts:12–30 supplies no login callback. The old cloudflare-stream.ts module is absent; cloudflare-auth.ts:22–50 now returns auth.baseUrl. Tests must invoke those real boundaries, not recreate removed APIs.

Fresh comparisons retain all provider descriptor fields, complete auth results and request bodies. Actual-source RED→GREEN6 now covers Copilot/Codex/OpenRouter fields, OpenCode Go name/API set, completion-token defaults, Cloudflare model-aware auth and Vertex ambient-only auth. [Exact proof and remaining inventory](../q15-sdk-provider-proof.md); not whole-barrel parity evidence.

## Staged APIs

Source-absent data-URL and timestamp helpers deleted. The removed model_runtime no longer consumes the timestamp; no fabricated source metadata or compatibility facade remains.

## Discrepancies

- None.
