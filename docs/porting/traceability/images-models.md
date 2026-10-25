# images-models traceability

## Q4 common-pin request auth

Closed against the whole common-pin module. Frozen `images-models.ts` resolves auth for the actual requested image model. Rust `get_auth(&ImagesModel)` and `generate_images` supply `AuthModel::Images` through the request resolver. `image_request_auth_receives_the_actual_image_model` checks a requested model absent from the listed catalog, compares actual frozen model-sensitive auth, and asserts the generated request receives its key. GREEN images_models9, revision `56d41700` + slice; ledger `/tmp/pi-rust-repin-delta-ai/ledger.md`.

Q15 deleted the provider-ID/status helper and get_auth_for_model alias. Auth/refresh return ModelsError directly. Frozen line212 reads Error.message; native auth failure generation now copies the exact JsString, proven by the actual-source raw-UTF-16 full-result test in [resolver proof](auth-resolve.md). Current closure and changed-boundary proof are below; historical evidence is not rerun evidence. [Auth contract](../../specs/auth-resolution.md).

Source: `packages/ai/src/images-models.ts`.
Rust: `crates/pi-ai/src/images_models.rs`.
Tests: `crates/pi-ai/tests/images_models.rs` (8 behavioral, ports the TS suite), `crates/pi-ai/tests/differential_images_models.rs` (registry reads, unknown provider, auth/env/header merge, baseUrl, explicit-option precedence vs pinned TS).

## Representation
- `ImagesProvider` → trait with `id/name/auth/get_models/supports_refresh_models/refresh_models/generate_images`. `refreshModels` method presence is `supports_refresh_models`.
- `ImagesModels` / `MutableImagesModels` → traits; `createImagesModels` → `create_images_models(CreateModelsOptions) -> ImagesModelsCollection`.
- Providers are stored in a `Vec<(id, Arc<dyn ImagesProvider>)>`; replacing an existing id keeps its original position, matching `Map.set`.
- `getModel` returns `Option<ImagesModel>` (owned clone) because TypeScript may return `undefined`.
- `generateImages` returns `BoxFuture<'static, Result<AssistantImages, CallbackError>>`; auth/authenticated-provider failures become `stopReason: "error"` results; no-auth async rejection propagates through Result, matching the frozen bug.
- `ImagesOptions.request` is the image-specific `ImagesRequestOptions`, because TypeScript parameterizes `ProviderRequestOptions` over `ImagesModel` and the chat `ProviderRequestOptions` carries chat-model callbacks.
- Dynamic refresh uses a shared `RefreshState` (`Notify` + outcome) so concurrent calls await one in-flight fetch; all-provider refresh uses a `JoinSet` and ignores failures.
- `refresh(Some(id))` returns ModelsError with code ModelSource, exact `Model refresh failed for {id}` message and separate original cause.

## Symbols / branches → proof
| TS | Rust | Test |
|---|---|---|
| `createImagesModels` (credentials/auth context defaults) | `create_images_models` | all behavioral tests |
| `setProvider` upsert order / `deleteProvider` / `clearProviders` | `MutableImagesModels` | `registers_providers_and_reads_models_synchronously` |
| `getProviders` / `getProvider` | same | `registers_providers_and_reads_models_synchronously`, differential `read` |
| `getModels()` all / per provider / unknown → `[]` | `get_models` | same, differential `read` |
| `getModel` found / missing | `get_model` | same, differential `read` |
| `getAuth(model)` | `get_auth(&ImagesModel)` | `resolves_auth_and_merges_it_into_requests_with_explicit_options_winning`, differential |
| auth failure → error result | `generate_images` error branch | `returns_error_results_for_unknown_providers_and_still_dispatches_unconfigured_auth` |
| unknown provider → `ModelsError("provider")` | `ModelsErrorCode::Provider` | same, differential `unknown` |
| unconfigured auth still dispatches | `resolution` `None` branch | same |
| auth `baseUrl` replaces the request model base URL | `request_model.base_url` | `applies_auth_base_url_and_headers_to_the_request_model`, differential |
| explicit api key wins; auth key otherwise | `.or(auth.api_key)` | `resolves_auth...`, differential |
| header merge per key, explicit wins, `null` retained | `merge_headers` | `applies_auth_base_url_and_headers...`, differential |
| env merge per key, explicit wins | `merge_env` | `merges_provider_resolved_env_into_image_options`, differential |
| other options preserved (`metadata`, extra, callbacks, fetch, signal, timeouts) | `options.clone()` + overrides | `options_metadata_and_extra_are_forwarded`, differential |
| `createImagesProvider` name default / auth / models / api | `create_images_provider` | `create_images_provider_defaults_name_to_id_and_keeps_auth` |
| `refreshModels` in-flight dedupe | `RefreshState` | `refreshes_dynamic_providers_with_in_flight_dedupe` |
| refresh failure keeps last-known models, later call retries | refresh `Err` branch | same (failure path asserted via error code/message) |
| `refresh(provider)` failure → `ModelsError("model_source")` | `ModelsError` | same |
| `refresh()` all providers best-effort | `refresh_all` + `JoinSet` | same |
| unknown/static provider refresh is a no-op | early returns | same |

## Common-pin closure (2026-10-25)

Whole-module source audit covers both exported factories, all collection operations, default credentials/context, provider order, requested-model auth, explicit-option precedence, optional refresh, in-flight dedupe, retained catalog and failure retry. Current12tests GREEN, `/tmp/ai3-images-green.log`. Empty baseUrl and no-auth rejection reproduced against the actual frozen import before repairs (`/tmp/ai3-images-baseurl-red.log`, `/tmp/ai3-images-reject-red-actual.log`); the earlier wrong-suite zero-test command and absent-target command are not proof. Existing ModelsError preservation reproduced RED (`/tmp/ai3-images-refresh-red.log`) then fixed through the same callback-original authority as chat model refresh. Mechanical cross-row change: make `models::wrap_refresh_error` crate-visible; no algorithm change. All collection callers are AI tests; no production consumer migration was needed.

| Classification | Boundary |
|---|---|
| DISCREPANCY, ported literally | Frozen says “Never rejects” but its no-auth `return provider.generateImages(...)` is not awaited in try/catch. Async rejection escapes; native Result now does the same. Authenticated await stays caught. |
| No consumer path | Frozen and native codex-image-generation use compat.streamSimple with a hosted image tool, not ImagesModels.generateImages. This repair does not change that tool’s user-visible catch path. |
| Approved native callback/runtime domain | A malicious native get_models implementation cannot throw a JavaScript exception through its infallible Vec trait; builtin image providers use snapshot reads. Browser readiness, dropped futures and exotic callback faults follow standing runtime gaps. |
| Owned-value approval | Catalog returns are owned; mutable JS reference identity is not reproduced. |

No pending whole-module feature. `builtinImagesModels` aggregation is already ported under providers/all; that separate row owns its implementation. [Contract](../../specs/images-models.md).

## Q18 provider request integration (2026-10-22)

Image auth resolution keeps native model endpoint, API key and environment values through request dispatch; environment merge has one ProviderEnv authority.

[Focused public-provider runtime proof](lossless-sdk-boundaries.md) records exact covered cases, revisions and remaining acceptance limits.

## Native metadata and model fields (2026-10-25)

Compile RED exposed scalar JSON metadata. ImagesOptions now forwards JsObject unchanged, including arbitrary nested JSON and unmatched units, through actual frozen/native generation callbacks. ImagesModel display names and unknown/historical properties use the existing JsString/ModelProperties authorities. Current13image tests GREEN (`/tmp/ai3-types-suite_modes.log`); the type witness also checks source-typed image records with exact unknown-key order. No new callback or property authority. See [stopped-lane handoff](../handoff-ai-core.md).
