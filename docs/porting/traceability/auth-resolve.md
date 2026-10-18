# auth resolve traceability

Source: `packages/ai/src/auth/resolve.ts` → `crates/pi-ai/src/auth/resolve.rs`.
Tests: `crates/pi-ai/tests/auth.rs`; differential: `tests/differential_ai_runtime` (`auth_resolve`).

## Symbols → proof
| TS | Rust | Test |
|---|---|---|
| `ModelsErrorCode`, `ModelsError` | same | `models_error_with_cause_keeps_detail_once`, OAuth failure tests |
| `AuthResolutionOverrides` | same | explicit key/env/min-validity tests |
| `resolveProviderAuth` | `resolve_provider_auth` | all resolution tests + differential |
| `withCauseDetail` | `with_cause_detail` | `oauth_refresh_failure_keeps_stored_credential_and_reports_oauth_code`, `models::wraps_credential_store_and_api_key_failures` |
| env overlay context | `OverlayAuthContext` | `explicit_request_api_key_and_env_resolve_provider_auth` (models) |
| `resolveStoredOAuth` double-checked refresh | same flow | `expired_oauth_refreshes_once_and_persists_rotated_credential`, `oauth_refresh_aborts_without_writing` |
| `resolveApiKey`, `readCredential` | same | `custom_credential_store_failures_wrap_with_auth_code`, `wraps_*` |

## Branches
- Explicit `apiKey` override only applies when the provider has api-key auth.
- A stored credential owns the provider: OAuth → OAuth handler, api-key → api-key handler, otherwise `None` (no ambient fallback).
- Ambient env resolution runs only when nothing is stored.
- OAuth refresh locks through `CredentialStore::modify`; the modifier re-checks expiry, refreshes with a 15 s timeout token, and persists the rotated credential. Failures preserve the stored credential and surface `ModelsError` code `oauth`.
- Explicit `minOAuthValidityMs` is checked again after refresh.
- Cancellation escapes as `ResolveError::Aborted`, never wrapped as an auth error.

## Gaps
- TS `AbortSignal.timeout` produces a `TimeoutError` DOMException; Rust uses a child token cancelled after 15 s, and the wrapped detail is `"The operation was aborted"`.
- `formatThrownValue` for non-Error values is not representable; callbacks expose only messages.

## Discrepancies
- None observed by the differential auth scenarios.
