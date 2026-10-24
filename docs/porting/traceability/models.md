# models runtime traceability

## Q15 getAuth header projection

Frozen models.ts:216–219 returns resolveProviderAuth unchanged. At6c1454e8, full actual-model auth differential RED proves native adds GitHub Copilot model.headers to the auth result. get_auth_for_model_inner no longer merges them. The original model retains headers for the API adapter; auth/options merging remains separate. models.rs regression asserts both boundaries. The ten-adapter wire-header matrix and zero-warning shared workspace check pass ([Q15 proof](../q15-sdk-provider-proof.md)); this does not certify the remaining old Models architecture.

## Q15 removed architecture

- Removed `tests/models.rs::always_gives_providers_a_concrete_signal_and_binds_store_waits_to_it` and its private RecordingStore fixture (parent clearance2026-10-22). It mutated a captured provider refresh-context signal after publication, testing source-absent ModelsStore/RefreshModelsContext/publication contracts. Frozen models.ts has none of those contracts. Removal is old-only deletion, not a weaker assertion or a reason-carrier backdoor. Production old architecture remains scheduled for the combined Q15 re-port.

## Abort fixture carrier checkpoint

Pi-ai test Fetch/auth/stream fixtures now accept shared AbortSignal; test abort owners use AbortController and pass signal() to consumers. Existing default-abort expectations stay unchanged. This is mechanical carrier compilation, not proof of reason forwarding, timeout branding, or complete abort parity. Those require separate frozen differentials.

## Q4 common-pin request auth

Partial. Frozen `models.ts` forwards the actual chat model to `resolveProviderAuth`; Rust `get_auth_for_model` and stream/complete request paths now do too, including model-aware provider composition. Behavioral actual-import test `request_auth_receives_the_actual_model_not_provider_status_context`: requested-2, not catalog first-1, reaches custom auth. GREEN 1, revision `56d41700` + slice, ledger `/tmp/pi-rust-repin-delta-ai/ledger.md`.

Provider-ID `get_auth` and status/refresh helpers explicitly pass no model; this is the approved Q15 status-only gap, never a fabricated-model fallback. Old publication/store/check/login/logout/deferred interfaces and current runtime-oracle removed imports remain unported; historical tables below are deletion inventory, not new-pin parity. [Auth contract](../../specs/auth-resolution.md).

Source: `packages/ai/src/models.ts` → `crates/pi-ai/src/models.rs`.
Tests: `crates/pi-ai/tests/models.rs`; differential `tests/differential_ai_runtime` (`cost`, `thinking`, `refresh`, `apply_auth`).

## Symbols → proof
| TS | Rust | Test |
|---|---|---|
| `ModelsPublication`, `PersistAction` | same (`PersistAction::{Unchanged,Delete,Write}`) | refresh/persistence tests |
| `RefreshModelsContext` | same (`publish` callback) | cache/network phase tests |
| `ModelsRefreshOptions`, `ModelsRefreshResult` | same | refresh tests |
| `Provider` | trait `Provider` (dyn-compatible) | test providers, `create_provider` |
| `Models` | trait `Models` | all models tests |
| `MutableModels` | trait `MutableModels` | `registers_replaces_and_deletes_providers` |
| `createModels` | `create_models` → `ModelsCollection` | all models tests |
| `ModelsImpl.setProvider/deleteProvider/clearProviders` | same | registration test |
| `getProviders/getProvider/getModels/getModel` | same | listing test |
| `mergeHeaders` | `merge_headers` | `merges_headers_case_insensitively_and_compares_models` |
| `refresh` | same | refresh, supersession, abort tests |
| `checkAuth`, `getAvailable` | same | auth check test |
| `getAuth` overloads | `get_auth` + `get_auth_for_model` | resolution tests |
| `login`, `logout` | same | login/logout test |
| `stream/complete/streamSimple/completeSimple` | same | stream/unknown-provider/auth-merge tests |
| `fetchDeferred/cancelDeferred` | same | deferred option test |
| `createProvider` | same → `Arc<dyn Provider>` | mixed-api, env, deferred, supersession tests |
| `hasApi` | `has_api` | listing test |
| `calculateCost` | `calculate_cost` | cost test + differential |
| `getSupportedThinkingLevels`, `clampThinkingLevel` | same | thinking test + differential |
| `modelsAreEqual` | `models_are_equal` | header/equality test |

## Concurrency semantics
- `refreshGenerations` and `refreshControllers`: `beginProviderRefresh` increments the generation and cancels the previous controller. A combined child token follows both the caller signal and the controller.
- Provider refresh work is spawned and raced against its signal. Abort stops waiting but leaves non-cooperative work running; its late error is ignored because the signal is aborted.
- Publication uses a per-provider promise-chain equivalent (`Notify` tail): publications run in call order, re-check signal/generation before and after persistence, and only then run the synchronous update. A superseded or aborted publication returns `false` without writing or updating.
- `refresh()` returns `{aborted, errors}`; cancellation is never reported as a provider error. Provider errors keep their message.
- OAuth refresh during model refresh runs under `CredentialStore::modify`, so concurrent refreshes rotate once.
- `login` starts the store mutation concurrently and distinguishes "aborted before the modifier ran" (returns abort) from "aborted after it started" (awaits the mutation, then maps cancellation).

## Extensions to binding contracts
- `ProviderRequestOptions` gained `transform_headers: Option<HeaderTransform>`. It is Models-only and stripped before provider dispatch (verified by the deferred/stream option tests and differential `apply_auth`).
- `calculate_cost` is pure (`&Usage -> UsageCost`) instead of mutating the usage record; the returned cost matches TS.

## Gaps / representation differences
- Provider `getModels()` cannot throw in Rust; the TS best-effort catch has no Rust branch.
- `ModelsRefreshResult.errors` maps to `CallbackError` messages; TS preserves arbitrary `Error` identity.
- `DeferredCancelOptions` is the `ProviderRequestOptions` alias with the added transform field.
- `getAuth` overloads become two methods (`get_auth`, `get_auth_for_model`).

## Discrepancies
- None observed by the differential scenarios (`cost`, `thinking`, `refresh`, `apply_auth`).

## Q18 provider request integration (2026-10-22)

Provider::base_url, provider endpoint config, auth request parts and applied auth use native endpoint/key/environment/header values. Static provider literals promote losslessly into that same authority.

[Focused public-provider runtime proof](lossless-sdk-boundaries.md) records exact covered cases, revisions and remaining acceptance limits.
