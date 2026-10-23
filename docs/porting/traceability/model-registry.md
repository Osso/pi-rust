# Model registry — Q15 re-port in progress

Frozen: `packages/coding-agent/src/core/model-registry.ts` at `4f9e6f8a4` (1001 lines).
Rust: `crates/pi-coding-agent/src/core/model_registry.rs`, internal `model_registry/schema.rs`.

The previous 158-line runtime facade described a source-absent architecture, not this pin.
The replacement owns AuthStorage and a synchronous model list; it does not delegate to ModelRuntime.
Consumer migration and acceptance remain open. This document is not a parity-completion claim.

| Frozen behavior | Rust | Proof |
|---|---|---|
| live `hasConfiguredAuth`, `getAvailable`, `isUsingOAuth` | synchronous AuthStorage reads | `model_registry_frozen::registry_observes_live_storage_without_a_refresh`; actual frozen oracle, old facade behavioral RED at 3de3f9e2; GREEN pending consumer migration |
| `create` / `inMemory`, `refresh`, `getAll`, `find`, `getError` | owned registry / synchronous reload | implementation present; differential matrix pending |
| custom model schema/defaults, overrides, mergeCompat, validation | internal schema + lossless JSON transformation | matrix pending |
| request auth/header precedence, provider status and display names | AuthStorage + config-value helpers | matrix pending |
| extension registration, unregister/reapply, OAuth/API registrations | registered config map, global compatibility registries | matrix pending |
| startup OpenRouter cache | production wiring pending | standalone catalog tests only |

## Remaining work

- Replace consumers of source-absent model_runtime, models_store, model_config,
  remote_catalog_provider and provider_composer; delete each module and its old-only tests.
- Complete model carrier compat unknown fields and real RPC serialization proof.
- Migrate SDK/session/UI/CLI/resolver consumers and run bounded differentials and all-target checks.
- Resolve invalid optional cached-model fields explicitly, preserving the approved gap.

No newly approved semantic deviations or classified frozen discrepancies.
