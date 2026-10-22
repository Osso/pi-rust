# auth resolve traceability

## Q4 common-pin request-model slice

Frozen `resolve.ts:30–67,120–132` at `4f9e6f8a4`: `AuthModel` is chat or image model and `resolveApiKey` passes that exact model. Rust request entrypoint now requires `AuthModel`; all chat/image request paths preserve it through provider composition. `models::request_auth_receives_the_actual_model_not_provider_status_context` and `images_models::image_request_auth_receives_the_actual_image_model` compare custom model-sensitive auth against actual frozen imports; requested models intentionally differ from the first catalog model. RED missing typed API → GREEN chat1/image9/auth17. Ledger `/tmp/pi-rust-repin-delta-ai/ledger.md`, revision `56d41700` + slice.

Approved Q15 staging gap: separately named `resolve_provider_status_auth` passes `None`, exclusively for provider-ID-only status/refresh wrappers. No request caller uses that path and no first-model/invented fallback exists. Gap documented at both API definitions and in [spec](../../specs/auth-resolution.md). Parent explicitly approved preserving this old-runtime status path until Q15.

**Partial: remaining old-only min-validity/timeout/cancellation behavior and flattened error causes below are NOT frozen parity.** Existing runtime differential imports removed modules and remains RED; it is not current proof. Frozen refresh checks expiry only (lines 89–106); Rust still refreshes preemptively. Frozen ModelsError stores cause separately; Rust still flattens detail. Historical mapping follows for deletion inventory only.

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

Q18 native auth integration and bounded proof status: [lossless auth](lossless-auth.md).
