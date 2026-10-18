# register-builtins (images) traceability

Source: `packages/ai/src/providers/images/register-builtins.ts`.
Rust: `crates/pi-ai/src/providers/images/register_builtins.rs`.
Tests: exercised by `crates/pi-ai/tests/images.rs` (`dispatches_through_the_registered_builtin_api`) and `crates/pi-ai/tests/openrouter_images_provider.rs`.

## Representation
- `loadOpenRouterImagesProviderModule()` dynamic import → direct module link; Rust cannot fail to load a statically linked module.
- `generateImagesOpenRouter` → `generate_images_openrouter`, delegating to `api::openrouter_images::generate_images`.
- `registerBuiltInImagesApiProviders()` → `register_built_in_images_api_providers()`; `ensure_built_in_images_api_providers()` applies it once (`OnceLock`) because Rust has no import-time side effect.

## Symbols / branches → proof
| TS | Rust | Test |
|---|---|---|
| `generateImagesOpenRouter` | `generate_images_openrouter` | `differential_openrouter_images` via `images` dispatch |
| lazy module cache / load error → error `AssistantImages` | direct link; no error branch | `dispatches_through_the_registered_builtin_api` |
| `registerBuiltInImagesApiProviders` | `register_built_in_images_api_providers` | `dispatches_through_the_registered_builtin_api` |
| import-time registration | `ensure_built_in_images_api_providers` | same |

## Gaps
- The lazy-load rejection branch (`createLazyLoadErrorImages`) has no Rust equivalent: statically linked modules cannot fail to import. This is an approved consequence of Rust-native extensions/static linking.

## Discrepancies
- None.
