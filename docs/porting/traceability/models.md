# models runtime traceability

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
