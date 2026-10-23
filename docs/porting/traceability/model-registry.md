# Model registry — Q15 re-port in progress

Frozen: `packages/coding-agent/src/core/model-registry.ts` at `4f9e6f8a4` (1001 lines).
Rust: `crates/pi-coding-agent/src/core/model_registry.rs`, internal `model_registry/schema.rs`.

The previous 158-line runtime facade described a source-absent architecture, not this pin.
The replacement owns AuthStorage and a synchronous model list; it does not delegate to ModelRuntime.
Consumer migration and acceptance remain open. This document is not a parity-completion claim.

| Frozen behavior | Rust | Proof |
|---|---|---|
| live `hasConfiguredAuth`, `getAvailable`, `isUsingOAuth` | synchronous AuthStorage reads | `model_registry_frozen::registry_observes_live_storage_without_a_refresh`; actual frozen oracle, old facade behavioral RED at 3de3f9e2; targeted GREEN after source/binary-harness migration (1 test); all-target consumers pending |
| `create` / `inMemory`, `refresh`, `getAll`, `find`, `getError` | owned registry / synchronous reload | implementation present; differential matrix pending |
| custom model schema/defaults, overrides, mergeCompat, validation | internal schema + lossless JSON transformation | matrix pending |
| request auth/header precedence, provider status and display names | AuthStorage + config-value helpers | matrix pending |
| extension registration, unregister/reapply, OAuth/API registrations | registered config map, global compatibility registries | matrix pending |
| startup OpenRouter cache | production wiring pending | standalone catalog tests only |

## Remaining work

- Complete test/example migration after deletion of source-absent model_runtime, models_store,
  model_config, remote_catalog_provider and provider_composer. Source consumers use ModelRegistry.
  Initial binding migration converts 39 unambiguous constructors and scripted stream fixtures to
  AuthStorage plus literal ProviderConfigInput. Remaining constructors, obsolete API assertions,
  UI/example differentials and all-target compilation are still open; this is not acceptance evidence.
- Old-only auth-check/auth-command/credential-print and background RPC/update-model refresh hooks are removed, not compatibility paths. The remaining auth-command metadata differential and update-model refresh tests are deleted with those APIs. Registry tests for arbitrary CredentialStore injection and registered-native-provider/config introspection are deleted: frozen registry accepts AuthStorage and exports none of those getters. Removed ModelConfig/FileModelsStore inspection tests are not lossless-registry proof; that matrix remains pending.
- Login test fixtures now register OAuthProviderInterface callbacks directly and pass AuthStorage, preserving prompt/manual-code/select, persistence, browser success/failure, unknown-provider/help and native error-unit assertions. Six focused tests pass (`q15-native-login-tests.log`); this is not whole frozen CLI differential acceptance.
- Complete model carrier compat unknown fields and real RPC serialization proof.
- Migrate SDK/session/UI/CLI/resolver consumers and run bounded differentials and all-target checks.
- Resolve invalid optional cached-model fields explicitly, preserving the approved gap.

## DISCREPANCY — catch String(error) can itself reject

Frozen `getApiKeyAndHeaders` catches an OAuth callback rejection, then evaluates
`error instanceof Error ? error.message : String(error)`. A thrown `{toString:0}`
therefore rejects its Promise with TypeError instead of resolving the auth-error union.
Port literally: `Result<ResolvedRequestAuth, CallbackError>` distinguishes that rejection
from `Ok(ResolvedRequestAuth::Err { error })` (parent confirmation 2026-10-23).
JS String conversion uses shared `pi-js-compat::js_json::to_js_string`.

`model_registry_auth_drift::provider_key_lookup_preserves_frozen_error_brand_and_arbitrary_thrown_value`
compares actual frozen Error properties, raw UTF-16 primitive rejection, and catch TypeError
against Rust. Compiler RED established the missing rejection channels; targeted GREEN covers
all three cases. `model_registry_oauth_throw_probe` independently records frozen behavior.
OAuth key callbacks return Result with CallbackError's AbortReason/NativeNodeError authority;
AuthStorage preserves the rejection unchanged. Broader registry matrices remain open.
