# images-models traceability

Source: `packages/ai/src/images-models.ts`.
Rust: `crates/pi-ai/src/images_models.rs`.
Tests: `crates/pi-ai/tests/images_models.rs` (8 behavioral, ports the TS suite), `crates/pi-ai/tests/differential_images_models.rs` (registry reads, unknown provider, auth/env/header merge, baseUrl, explicit-option precedence vs pinned TS).

## Representation
- `ImagesProvider` → trait with `id/name/auth/get_models/supports_refresh_models/refresh_models/generate_images`. `refreshModels` method presence is `supports_refresh_models`.
- `ImagesModels` / `MutableImagesModels` → traits; `createImagesModels` → `create_images_models(CreateModelsOptions) -> ImagesModelsCollection`.
- Providers are stored in a `Vec<(id, Arc<dyn ImagesProvider>)>`; replacing an existing id keeps its original position, matching `Map.set`.
- `getModel` returns `Option<ImagesModel>` (owned clone) because TypeScript may return `undefined`.
- `generateImages` returns `BoxFuture<'static, AssistantImages>`; provider/auth failures become `stopReason: "error"` results, never rejections.
- `ImagesOptions.request` is the image-specific `ImagesRequestOptions`, because TypeScript parameterizes `ProviderRequestOptions` over `ImagesModel` and the chat `ProviderRequestOptions` carries chat-model callbacks.
- Dynamic refresh uses a shared `RefreshState` (`Notify` + outcome) so concurrent calls await one in-flight fetch; all-provider refresh uses a `JoinSet` and ignores failures.
- `refresh(Some(id))` maps failures to `ResolveError::Models(ModelsError { code: ModelSource })` with `Model refresh failed for {id}: {cause}`.

## Symbols / branches → proof
| TS | Rust | Test |
|---|---|---|
| `createImagesModels` (credentials/auth context defaults) | `create_images_models` | all behavioral tests |
| `setProvider` upsert order / `deleteProvider` / `clearProviders` | `MutableImagesModels` | `registers_providers_and_reads_models_synchronously` |
| `getProviders` / `getProvider` | same | `registers_providers_and_reads_models_synchronously`, differential `read` |
| `getModels()` all / per provider / unknown → `[]` | `get_models` | same, differential `read` |
| `getModel` found / missing | `get_model` | same, differential `read` |
| `getAuth(providerId)` / `getAuth(model)` | `get_auth` / `get_auth_for_model` | `resolves_auth_and_merges_it_into_requests_with_explicit_options_winning`, differential |
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
| `refresh(provider)` failure → `ModelsError("model_source")` | `ResolveError::Models` | same |
| `refresh()` all providers best-effort | `refresh_all` + `JoinSet` | same |
| unknown/static provider refresh is a no-op | early returns | same |

## Gaps
- `builtinImagesModels` lives in `providers/all.ts`, outside this slice; the OpenRouter provider function is ported, but the `all.ts` aggregate remains unported.
- TypeScript's `getModels` catches providers whose `getModels()` throws. The typed Rust trait returns `Vec<ImagesModel>` and cannot throw, so no catch branch exists.
- `ModelsError` instances thrown by a provider's own `refreshModels` are rethrown unchanged in TypeScript. The Rust refresh callback returns `CallbackError`, so every failure is wrapped as `model_source`; no current provider produces a `ModelsError` from refresh.
- `getAuth(model)` in Rust takes `&ImagesModel`; TypeScript's `ImagesModel<ImagesApi>` generic is erased the same way.

## Discrepancies
- None.

## Q18 provider request integration (2026-10-22)

Image auth resolution keeps native model endpoint, API key and environment values through request dispatch; environment merge has one ProviderEnv authority.

[Focused public-provider runtime proof](lossless-sdk-boundaries.md) records exact covered cases, revisions and remaining acceptance limits.
