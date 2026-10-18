# auth types traceability

Source: `packages/ai/src/auth/types.ts` → `crates/pi-ai/src/auth/types.rs`.
Tests: `crates/pi-ai/tests/auth.rs`.

## Representation decisions
- `Credential` is an enum with manual serde: `type` is written first, `expires` uses JavaScript-number formatting, and unknown OAuth keys are preserved in `OAuthCredential::extra` (`Map<String, Value>`).
- `CredentialInfo` uses camelCase `providerId` and a `type` tag.
- `AuthOperationOptions.signal` → `Option<CancellationToken>`.
- `AuthContext`, `ApiKeyAuth`, `OAuthAuth`, `CredentialStore`, and `AuthInteraction` are dyn-compatible; owned inputs use `Arc` so their futures are `'static`.
- `AuthPrompt` keeps `signal` plus a `AuthPromptKind` enum for the four prompt shapes.
- `AuthEvent` is a serde-tagged enum (`info`, `auth_url`, `device_code`, `progress`).
- `AuthError::{Aborted, Failed(CallbackError)}` keeps cancellation distinct from callback/storage failure. `ResolveError` adds `Models(ModelsError)` and raw `Callback(CallbackError)` for login failures TypeScript propagates unwrapped.
- `ProviderAuth` holds `Option<Arc<dyn ApiKeyAuth>>` / `Option<Arc<dyn OAuthAuth>>`.

## Symbols → proof
| TS | Rust | Test |
|---|---|---|
| `ModelAuth`, `ApiKeyCredential`, `OAuthCredential`, `Credential`, `CredentialInfo` | same names | `credential_round_trips_and_preserves_extra_oauth_keys`, `in_memory_store_lists_metadata_and_serializes_modifications` |
| `AuthResult`, `AuthCheck`, `AuthType` | same | `auth_result_serializes_provider_env_and_source`, `models_error_with_cause_keeps_detail_once` |
| `AuthPrompt`, `AuthEvent`, `AuthInteraction` | same | `env_api_key_login_prompts_for_a_secret` |
| `ApiKeyAuth` (`name/login/check/resolve`) | trait | `env_api_key_auth_*`; models tests exercise `check/resolve/login` |
| `OAuthAuth` (`login/refresh/toAuth`, subscription/label) | trait | `lazy_oauth_loads_implementation_once_and_delegates`, OAuth tests |
| `CredentialStore` (`read/list/modify/delete`) | trait | credential-store and resolve tests |
| `ProviderAuth` | struct | all resolution tests |

## Gaps
- `AbortSignal.reason` is not representable; cancellation is only observable as `AuthError::Aborted` / `ResolveError::Aborted`.
- `ProviderAuthInteraction` is not a separate type: providers call `AuthInteraction::signal()`, which returns a concrete token (a fresh non-cancelled token when the caller omitted one).
- OAuth extra keys are stored in insertion order via `serde_json` `preserve_order`; other maps (`ProviderEnv`, `ProviderHeaders`) are `BTreeMap` and serialize sorted.

## Discrepancies
- None in resolution semantics. Typed Rust rejects non-object credential JSON and unknown `type` values instead of returning an untyped object.
