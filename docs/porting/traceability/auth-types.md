# Auth types — common pin

Authority: `packages/ai/src/auth/types.ts` (182 lines), [4f9e6f8a4](../source-lock.json). Rust: `crates/pi-ai/src/auth/types.rs`. [Contract](../../specs/auth-types.md).

## Complete symbol map

| Frozen export | Native representation / proof |
|---|---|
| ModelAuth, ApiKeyCredential, OAuthCredential, Credential | Native UTF-16 fields, tagged Credential enum, OAuth extra JsObject; credential roundtrip/native-boundary tests and cross-runtime auth.json bytes. |
| CredentialStore | Only read/modify/delete; CredentialModifier carries callback results. Compiler RED15 → current positive Rust contract and actual frozen type fixture. Owned copies follow the approved general value-ownership rule. |
| AuthContext | env/file_exists return fallible native futures; injected values/path arguments remain JsString, environment names retain the established scalar domain. Actual Anthropic/Vertex rejection/call-order matrix. |
| AuthResult | auth/env/source with exact native text; auth and model/image request tests. |
| AuthPrompt | AuthPrompt plus AuthPromptKind and AuthSelectOption; text/secret/select/manual_code, optional per-prompt signal, native messages/options/answers. Provider login differentials and lossless interaction tests. |
| AuthEvent | Only auth_url/device_code/progress; optional numeric fields use JS numbers. All three event JSON shapes retain raw UTF-16 in lossless_auth_interaction. |
| AuthLoginCallbacks | AuthInteraction: prompt future and synchronous notify return shared callback failures; concrete non-aborted signal represents omission. Supplied signal identity/reasons use the shared carrier. |
| ApiKeyAuth | JsString name, Option<ApiKeyLogin> callback, required actual AuthModel chat/image input. No check/signal/display compatibility interface. Native and actual-source type contracts; provider/request-model tests. |
| OAuthAuth | JsString name, login/refresh/to_auth; no refresh signal, subscription flag or login label. Existing three frozen flow regressions; raw callback proof. |
| ProviderAuth | Optional Arc-backed api_key/oauth handlers; models/providers/image dispatch matrix. |

AuthError remains a boxed shared CallbackError carrier; CallbackError's authority is now `pi_js_compat::callback_error` (pi-ai re-export unchanged). No lane-local throw transport or panic carrier.

## Proof ledger

All logs: `/tmp/pi-rust-repin-delta-ai/`; exact history: `q15-proof-ledger.md` there.

| Boundary | Evidence |
|---|---|
| Actual frozen public interfaces | `oracle/auth-types-contract.mts` typechecks the real frozen exports, including exact store/method/event keys; no patched source or lint suppression. |
| Core auth/models/providers/images | 93 tests GREEN across ten bounded targets: `q15c-auth-current-contract-regression.log`. This is not a whole-workspace runtime suite. |
| Context/notify failures | Compiler RED3 → 21 real-provider cases: six raw reasons plus actual frozen abort(undefined) → branded AbortError/code20 for each of env/file/notify. Native reason branding, full thrown value and call order asserted. Current carrier integration GREEN5: `q15c-auth-carrier-provider-green.log` (one child-entry guard). |
| App modifier failure | Real frozen InMemoryCredentialStore comparison exposed null→new Error stringification; StorageError::from_auth preserves all six reasons and unchanged state. `q15c-auth-store-modifier-rejection-{red,green}.log`; post-carrier GREEN1 in `q15c-auth-carrier-modifier-green.log`. |
| App lock/storage lifecycle | Phase1 3, process4, rotation3, storage14 GREEN in `q15c-auth-app-regression.log`; its obsolete read-waits expectation failed afterward. Actual source proof established finite sync retries/absent snapshot. Explicit release-file barriers now compare frozen withLockAsync/native modify, retaining credential equality/minimum wait/reverse ELOCKED. Cross-process3 + runtime3 GREEN: `q15c-auth-lock-and-runtime-green.log`. |
| OAuth/native UTF-16 | Anthropic6/GitHub12 GREEN in `q15c-auth-oauth-native-regression.log`; lossless interaction10 GREEN in its follow-up. Codex8/registry3/public-index2/remaining SDK3 GREEN: `q15c-auth-oauth-native-regression-3.log`. Old-only Radius fixture is not frozen parity evidence. |
| Integration | Workspace/all-target check zero errors/warnings after parent a913c452: `q15c-auth-workspace-carrier-integrated.{jsonl,stderr}`. Oracle tsgo/Biome six-file checks GREEN: `q15c-auth-oracle-{typecheck,lint}-green.log`. Final changed-line clippy/inventory recorded at checkpoint. |

## DISCREPANCY preserved

Frozen `utils/oauth/openai-codex.ts:490` invokes onAuth before try/finally493–569. A notify throw leaks its browser listener; the outer auth wrapper still aborts manual input. Real isolated-process RED showed native incorrectly closing it. Native now preserves the source bug; rejection value, select/auth_url ordering and occupied port1455 match. Oracle/native child exit releases listeners. No host listener remains after the test. [Flow mapping](openai-codex.md).

Interactive UI uses an infallible inherent notifier. Its Result-returning AuthInteraction method returns Ok after that call; legacy void callbacks call the inherent method directly. No caught login error is routed to uncaught handling.

## Exclusions

| Domain | Governing decisions |
|---|---|
| JS-only non-JSON callback rejections (undefined, Symbol, BigInt) | Rust-native extensions (2026-10-18) and typed-core invalid-JS-runtime-type exclusion (2026-08-12), reaffirmed by parent2026-10-24. Native callbacks reject via shared CallbackError/AbortReason (JSON/native Error/DOMException). JsValue has no Undefined variant; separately, abort(None) models omitted/undefined, never null, and is tested against real abort(undefined). |
| Invalid typed records / JS alias mutation | Existing typed-core and owned-value approvals; valid values remain exact. Non-object credentials and unknown credential/event tags fail explicitly. Environment/header key domains follow [lossless-content](../../specs/lossless-content.md). |

## Explicitly retired old-only tests/assertions

| File | Removed surface |
|---|---|
| pi-ai/tests/auth.rs | in_memory_store_lists_metadata_and_serializes_modifications; aborted_queued_mutation_never_runs; AuthCheck serialization only from retained ModelsError test. |
| pi-ai/tests/auth_abort_reasons.rs | credential_abort_retains_the_actual_native_error_message; every_credential_operation_retains_default_falsey_raw_and_native_abort_reasons; queued_modify_and_delete_abort_with_their_own_reason_without_mutation. Operation cancellation options are absent from frozen stores. |
| pi-ai/tests/provider_auth.rs / providers.rs | anthropic_resolve_honors_abort_before_env_reads; oauth_subscription_flags_match_the_ts_factories. Missing-model fixtures now supply actual requested models. |
| pi-coding-agent/tests/auth_storage.rs | pre_aborted_operations_do_not_create_the_file_or_run_the_mutation; aborts_while_waiting_for_a_held_file_lock_without_running_the_mutation; in_memory_backend_rejects_aborted_operations; list-only assertions from otherwise retained state tests. |
| pi-coding-agent/tests/runtime_credentials.rs | enumeration_merges_overrides_without_exposing_keys; forwards_operation_signals_to_the_persistent_store. Supplied-false backend rejection/override preservation retained. |
| pi-coding-agent/tests/auth_phase1_rotation.rs | Only old operation-signal cancel subcase; renamed successful_rotation_callback_then_cancel_or_write_failure_rescues_exact_result to successful_rotation_callback_then_write_failure_rescues_exact_result. Write/compromise rescue retained. |
| pi-ai/tests/lossless_auth_interaction.rs | Info-only auth_event_json_retains_raw_prompt_related_text replaced by all three frozen event shapes with exact UTF-16. |
| pi-ai/tests/lossless_sdk_boundaries.rs | vertex_login_retains_raw_project_location_and_credential_path and sole Answers helper: frozen Vertex is ambient-only; actual factory proof retained. |
| OAuth/provider tests and descriptor oracles | Subscription/login-label assertions and synthetic descriptor fields removed; real name/login/refresh/request/credential assertions retained. |

AuthType was not restored as an auth export. The real interactive login-method discriminator is LoginAuthType in login_options.rs.

## Separate remaining slices

Whole helper/context/store implementation revalidation is tracked in their own rows. Source-absent OpenRouter/Radius/Kimi/xAI OAuth modules/load overrides/exports and OpenRouter image OAuth advertisement are the explicitly approved next slice, after outside-consumer audit. This interface closure does not claim those modules match frozen. Parent's known-RED external-editor lib test and final milestone verification remain outside this lane's checkpoint.

## OAuth credential property order (2026-10-25)

OAuthCredential adds key_order metadata, captured from native JSON before extracting typed fields. Serialization drains keys in that order and appends newly added fields; values remain canonical typed access/refresh/expires plus extras. `.new` retains its explicit native construction order; Codex assigns the frozen access/refresh/expires/accountId/email order. App storage separately reproduces the frozen type-first spread. This is persistence parity, not a fixed-order deviation. Native interleaved-key round-trip and actual frozen AuthStorage/Codex refresh file-byte proof are in `tests/auth_codex_credential_order.rs` in the app crate.
