# openrouter-images provider traceability

Source: `packages/ai/src/providers/openrouter-images.ts`.
Rust: `crates/pi-ai/src/providers/openrouter_images.rs`.
Tests: `crates/pi-ai/tests/openrouter_images_provider.rs` (4 behavioral).

## Representation
- `openrouterImagesProvider()` → `openrouter_images_provider() -> Arc<dyn ImagesProvider>`.
- `id`/`name`/`models`/`api` map directly; models come from `get_image_models("openrouter")` (the embedded generated catalog, insertion order preserved).
- `auth.apiKey` → `env_api_key_auth("OpenRouter API key", ["OPENROUTER_API_KEY"])`.
- `auth.oauth` → `lazy_oauth` with the same name/login label. The loader for `auth/oauth/openrouter.ts` is outside this slice, so it returns an explicit `AuthError` instead of loading an implementation. No stored OAuth credential exists unless an app writes one; normal api-key resolution never invokes the loader.
- `api` → `openrouter_images_api()` (direct link to `OpenRouterImagesApi`).

## Symbols / branches → proof
| TS | Rust | Test |
|---|---|---|
| `createImagesProvider` metadata | `openrouter_images_provider` | `provider_metadata_and_catalog_match_the_generated_catalog` |
| `models: Object.values(IMAGE_MODELS.openrouter)` | `get_image_models("openrouter").to_vec()` | same (42 models, order) |
| `envApiKeyAuth` key + `OPENROUTER_API_KEY` | `env_api_key_auth` | `api_key_auth_resolves_openrouter_api_key` |
| unconfigured provider → no auth | api-key resolve `None` | `unconfigured_provider_reports_no_auth` |
| `lazyOAuth` metadata | `lazy_oauth` | `provider_metadata...` (oauth present) |
| `openrouterImagesApi()` | `openrouter_images_api()` | used by dispatch tests |
| builtin collection shape | `create_images_models` + `set_provider` | `builtin_style_collection_exposes_the_openrouter_catalog` |

## Gaps
- The OpenRouter OAuth flow module (`auth/oauth/openrouter.ts`) is not part of this slice. Its loader returns `OpenRouter OAuth flow is not ported (auth/oauth/openrouter.ts)`; OAuth login/refresh for this provider is therefore unavailable until that module is ported. API-key auth is complete.
- `providers/all.ts` (`builtinImagesProviders`/`builtinImagesModels`) is a separate module and remains unported; the provider itself and the equivalent collection setup are tested.

## Discrepancies
- None.
