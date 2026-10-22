# model-registry

Q18 native error consumers: ResolvedRequestAuth::Err.error and its accessor retain JsString; get_error preserves native runtime errors. Existing no-API-key modifier applies only to the exact source sentinel; arbitrary provider human text is never Display-projected. Runtime proof remains blocked by concurrent coding-library dependencies.

Source: `packages/coding-agent/src/core/model-registry.ts` (158 lines).
Rust: `crates/pi-coding-agent/src/core/model_registry.rs`.
Tests: `crates/pi-coding-agent/tests/model_registry.rs` (ports
`test/model-registry-resolved-auth.test.ts` and registry-facade cases from
`test/model-registry.test.ts`).

## Exports and branches

| TS export | Rust item | Test |
|---|---|---|
| `ProviderConfigInput` re-export | `provider_composer::ProviderConfigInput` | registry tests |
| `ResolvedRequestAuth` union | `ResolvedRequestAuth` enum | `exposes_model_runtime_auth_resolution_*`, compatibility tests |
| `clearApiKeyCache` re-export | `clear_api_key_cache` | covered by provider_composer |
| `formatRequestAuthError` authHeader special case | `format_request_auth_error` | `get_api_key_and_headers_preserves_the_missing_key_auth_header_error` |
| `refresh/getError/getAll/getAvailable/find/hasConfiguredAuth` | same | `refresh_reloads_custom_models_from_disk`, auth/oauth test |
| `getApiKeyAndHeaders` (resolution, compatibility fallback, error mapping) | same | five `get_api_key_and_headers_*` tests |
| `getProviderAuthStatus/getProvider/complete` | same | `provider_auth_status_*` |
| `getProviderDisplayName` | same | `provider_display_names_*` |
| `getProviderAuth/getApiKeyForProvider/resolveProviderAuth` | same | `accepts_a_pi_ai_credential_store`, `get_api_key_for_provider_*` |
| `isUsingOAuth` | same | `has_configured_auth_and_oauth_*` |
| `registerProvider` overloads | `register_native_provider` + `register_provider(name, config)` | `registered_provider_accessors_round_trip` |
| `unregisterProvider` | same | same |
| registered config/native/ids accessors | same | same |

## Gaps / deviations

- The TypeScript overload `registerProvider(provider)` / `registerProvider(name, config)`
  is split into two methods (typed Rust has no overloads).
- `complete` is implemented but not exercised (would need a stream provider).

## Discrepancies

None found.

Q18 auth consumer propagation retains native key/base URL/env/header values; native errors avoid Display except explicit formatting. Tests/proof status: [lossless auth](lossless-auth.md).

## DRIFT: provider-key lookup swallows auth failure

Frozen `packages/coding-agent/src/core/model-registry.ts:811-820` awaits authStorage.getApiKey without a catch. Rust `get_api_key_for_provider` currently uses `.ok().flatten()`, losing a rejection. Per parent sequencing, retain that production behavior until coding-agent compiles and a public-boundary RED can run; then migrate it and codex_usage:414 together to `Result<Option<JsString>, ResolveError>`. Required public-boundary test `model_registry_auth_drift::provider_key_lookup_propagates_the_native_public_auth_rejection` is written, **unexecuted**. This is not approved parity.
