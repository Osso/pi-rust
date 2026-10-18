# images traceability

Source: `packages/ai/src/images.ts`.
Rust: `crates/pi-ai/src/images.rs`.
Tests: `crates/pi-ai/tests/images.rs` (2 behavioral: builtin dispatch, unknown API).

## Representation
- `generateImages(model, context, options?)` → `generate_images(...) -> BoxFuture<'static, Result<AssistantImages, CallbackError>>`.
- The TypeScript module registers bundled APIs through `import "./providers/images/register-builtins.ts"`; Rust has no import side effect, so `generate_images` calls `ensure_built_in_images_api_providers()` (`OnceLock`) before lookup.
- `resolveImagesApiProvider` throwing → `Err(CallbackError)` with `No API provider registered for api: {api}`.
- Provider dispatch reuses `images_api_registry::get_images_api_provider`.

## Symbols / branches → proof
| TS | Rust | Test |
|---|---|---|
| import-time builtin registration | `ensure_built_in_images_api_providers` | `dispatches_through_the_registered_builtin_api` |
| `resolveImagesApiProvider` missing → throw | `Err(CallbackError)` | `unknown_apis_are_reported` |
| `provider.generateImages(...)` dispatch | registry lookup + delegate | `dispatches_through_the_registered_builtin_api`, `differential_openrouter_images` (through the API module) |

## Gaps
- No standalone differential: the module is a thin dispatcher over a process-global registry; its observable behavior is covered by the behavioral tests and the OpenRouter API differential.

## Discrepancies
- None.
