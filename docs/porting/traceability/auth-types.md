# auth types traceability

## Q15c active replacement

Core API replacement in progress; **not an integration checkpoint**. `auth_types_contract` has a genuine15-error compiler RED against the previous API and GREEN2 against the replacement: required actual model, bare read/modify/delete methods, UTF-16 auth names, and optional login-function presence. Removed old-only list/check/operation-options/display/Info contracts from the core; coding-agent adapters and historical tests still need migration before workspace proof. `q15c-auth-types-{red,core-green}.log` records this boundary. [Current contract](../../specs/auth-types.md).

## Removed old-only tests and assertions

| File | Retired source-absent surface |
|---|---|
| `pi-ai/tests/auth.rs` | Deleted `in_memory_store_lists_metadata_and_serializes_modifications`, `aborted_queued_mutation_never_runs`; removed only AuthCheck serialization from the retained ModelsError cause test. |
| `pi-ai/tests/auth_abort_reasons.rs` | Deleted `credential_abort_retains_the_actual_native_error_message`, `every_credential_operation_retains_default_falsey_raw_and_native_abort_reasons`, `queued_modify_and_delete_abort_with_their_own_reason_without_mutation`: operation-option cancellation is absent from frozen stores. Shared raw/abort callback carriers remain covered elsewhere. |
| `pi-ai/tests/provider_auth.rs`, `providers.rs` | Deleted `anthropic_resolve_honors_abort_before_env_reads`, `oauth_subscription_flags_match_the_ts_factories`. Required actual-model fixtures replace missing-model inputs. |
| `pi-coding-agent/tests/auth_storage.rs` | Deleted `pre_aborted_operations_do_not_create_the_file_or_run_the_mutation`, `aborts_while_waiting_for_a_held_file_lock_without_running_the_mutation`, `in_memory_backend_rejects_aborted_operations`; removed store-list assertions only from retained read/modify/delete tests. |
| `pi-coding-agent/tests/runtime_credentials.rs` | Deleted `enumeration_merges_overrides_without_exposing_keys`, `forwards_operation_signals_to_the_persistent_store`; retained actual supplied-false backend rejection/override persistence proof. |
| `pi-coding-agent/tests/auth_phase1_rotation.rs` | Removed only the operation-signal `cancel` subcase and renamed `successful_rotation_callback_then_cancel_or_write_failure_rescues_exact_result` to `successful_rotation_callback_then_write_failure_rescues_exact_result`; write/compromise rescue cases remain. |
| OAuth/provider tests and `provider-module-oracle.ts`/`providers-oracle.ts` | Removed subscription/login-label assertions and synthetic descriptor fields, absent from frozen auth/types and lazyOAuth; retained real names/login/refresh/request/credential comparisons. |

`AuthType` was not restored as an auth export: the real frozen interactive login-method discriminator lives as `LoginAuthType` in `login_options.rs`, with selector consumers migrated mechanically.

## Historical Q4 common-pin status

Partial. Frozen `auth/types.ts:148–158` requires the chat/image model on every API-key resolve input. `AuthModel::{Chat,Images}` now models that union; request entrypoints supply the actual requested model (chat/image actual-import oracle tests). The provider-status resolver is deleted. ApiKeyResolveInput.model remains optional in this not-yet-rewritten types module; resolver requests always provide Some(actual model), never a fabricated representative.

Current proof: models request-model test1, images_models9, auth17 GREEN; revision `56d41700` + slice, ledger `/tmp/pi-rust-repin-delta-ai/ledger.md`. Remaining old-only list/check/operation-option/OAuth display flags/Info event interfaces still require deletion. The historical representation/tests below are not common-pin parity for those exports. [Contract](../../specs/auth-resolution.md).

Source: `packages/ai/src/auth/types.ts` → `crates/pi-ai/src/auth/types.rs`.
Tests: `crates/pi-ai/tests/auth.rs`.

## Representation decisions
- `Credential` is an enum with manual serde: `type` is written first, `expires` uses JavaScript-number formatting, and unknown OAuth keys are preserved in `OAuthCredential::extra` (`Map<String, Value>`).
- `CredentialInfo` uses camelCase `providerId` and a `type` tag.
- `AuthOperationOptions.signal` uses the [shared reason-bearing carrier](../../specs/abort-signals.md).
- `AuthContext`, `ApiKeyAuth`, `OAuthAuth`, `CredentialStore`, and `AuthInteraction` are dyn-compatible; owned inputs use `Arc` so their futures are `'static`.
- `AuthPrompt` keeps `signal` plus a `AuthPromptKind` enum for the four prompt shapes.
- `AuthEvent` is a serde-tagged enum (`info`, `auth_url`, `device_code`, `progress`).
- `AuthError::{Aborted(Box<CallbackError>), Failed(Box<CallbackError>)}` retains the actual shared reason, including native error sources, raw falsy values and UTF-16. The resolver has no separate abort channel: its catches wrap these values according to the [resolver contract](auth-resolve.md).
- `ProviderAuth` holds `Option<Arc<dyn ApiKeyAuth>>` / `Option<Arc<dyn OAuthAuth>>`.

## Symbols → proof
| TS | Rust | Test |
|---|---|---|
| `ModelAuth`, `ApiKeyCredential`, `OAuthCredential`, `Credential`, `CredentialInfo` | same names | `credential_round_trips_and_preserves_extra_oauth_keys`, `in_memory_store_lists_metadata_and_serializes_modifications` |
| `AuthResult`, `AuthCheck`, `AuthType` | same | `auth_result_serializes_provider_env_and_source`, `models_error_with_cause_keeps_detail_once` |
| `AuthPrompt`, `AuthEvent`, `AuthInteraction` | same | `env_api_key_login_prompts_for_a_secret` |
| `ApiKeyAuth` (`name/login/check/resolve`, `supports_login`) | trait | `env_api_key_auth_*`; models tests exercise `check/resolve/login`; `login_options::tests::api_key_login_availability_follows_the_auth_method` covers `supports_login` (ambient-only `FauxApiKeyAuth` reports `false`) |
| `OAuthAuth` (`login/refresh/toAuth`, subscription/label) | trait | `lazy_oauth_loads_implementation_once_and_delegates`, OAuth tests |
| `CredentialStore` (`read/list/modify/delete`) | trait | credential-store and resolve tests |
| `ProviderAuth` | struct | all resolution tests |

## Gaps
- Reason loss is removed: `auth_abort_reasons` tests every credential operation and actual queued modify/delete cancellation; `auth_storage` checks memory/file lock cancellation and native error sources; `runtime_credentials` checks override preservation after a backend's supplied-false abort. `oauth_abort_wrapper` imports frozen `utils/oauth/index.ts:157–159` and compares default DOMException, false, raw UTF-16, failed JS ToString and native Error/cause metadata. The shared callback helper delegates String(value) exclusively to pi-js-compat; source catch wrapping retains the original cause. These prerequisite proofs do not establish whole auth/types.ts or auth/resolve.ts parity.
- `ProviderAuthInteraction` is not a separate type: providers call `AuthInteraction::signal()`, which returns a concrete token (a fresh non-cancelled token when the caller omitted one).
- OAuth extra keys are stored in insertion order via `serde_json` `preserve_order`. `ProviderEnv` and `ProviderHeaders` are `IndexMap`: TypeScript object insertion order is the provider request order and must survive auth application (`differential_ai_runtime` apply-auth scenario compares merged headers/env in order).

## Discrepancies
- None in resolution semantics. Typed Rust rejects non-object credential JSON and unknown `type` values instead of returning an untyped object.

## Q18 native login contracts (compiler checkpoint)

AuthInteraction prompt answers, prompt/event/link/choice text, credential keys, auth request key/base URL and source labels now retain `JsString`; OAuth legacy callbacks share native text. Credential serialization/deserialization uses one `JsValue` authority, no scalar serde intermediate. `lossless_auth_interaction` first reproduced scalar credential rejection (behavioral RED), then missing native prompt signature (compiler RED). Consumer propagation and real HTTP boundary proof remain pending; no completion claim.

Q18 native auth integration and bounded proof status: [lossless auth](lossless-auth.md).
