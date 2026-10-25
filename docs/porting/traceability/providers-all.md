# providers/all — common pin

Frozen `packages/ai/src/providers/all.ts` at 4f9e6f8a4; native `providers/all.rs`. Entire module complete under the user-observable parity bar. Imported provider/API modules have their own coverage rows.

| Frozen exports / branch | Proof |
|---|---|
| getBuiltinModel / getBuiltinProviders / getBuiltinModels | Current `differential_chat_catalog`: all 36 provider keys, 1058 full ordered model records, known/unknown lookups. |
| builtinProviders | `differential_provider_inventory`: exact36 IDs/order; `differential_providers`: all descriptors and full auth/dispatch comparisons, including default/GC/Team Codex aliases. |
| builtinModels | New actual-source collection differential compares every registered model value/order and provider registration. Clearing a separately constructed collection leaves the observed collection intact. |
| builtinImagesProviders / builtinImagesModels | Same full collection differential; actual OpenRouter provider registration and all image-model values. |
| Owned native values | Approved 2026-10-22 general ownership rule. Source consumer audit: direct helpers are only aliased by compat.ts; coding-agent model-registry.ts:442/594 reads catalog values and uses object spreads for overrides, while model-catalog-cache.ts:195 inserts values into a new Map without mutating records. No built-in mutation-through-returned-reference dependency. |

Source-absent factories/data-URL/timestamp helpers were deleted in earlier committed slices. No fallback or new compatibility API was added. Current proof: `/tmp/ai-provider-batch-baseline.log`, `/tmp/ai-provider-collections.log`.
