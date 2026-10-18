# images-api-registry traceability

Source: `packages/ai/src/images-api-registry.ts`.
Rust: `crates/pi-ai/src/images_api_registry.rs`.
Tests: `crates/pi-ai/tests/images_api_registry.rs` (3 behavioral).

## Representation
- `ImagesApiProvider { api, generateImages }` → `ImagesApiProvider { api, generate_images: Arc<dyn ProviderImages> }`.
- `registerImagesApiProvider(provider, sourceId?)` → `register_images_api_provider(provider)`. `sourceId` is stored by TypeScript but never returned by `getImagesApiProvider`; it is an unobservable optional argument, so the typed Rust API excludes it.
- The module-level `Map` → `LazyLock<Mutex<BTreeMap<String, Arc<dyn ProviderImages>>>>`.
- `wrapGenerateImages` (api mismatch throws) → `RegisteredImagesApiProvider`, which returns `Err(CallbackError)` with the same message.

## Symbols / branches → proof
| TS | Rust | Test |
|---|---|---|
| `registerImagesApiProvider` (overwrite by api) | `register_images_api_provider` | `registers_and_resolves_by_api`, `later_registrations_replace_earlier_ones` |
| `getImagesApiProvider` (unknown → `undefined`) | `get_images_api_provider` → `Option` | `registers_and_resolves_by_api` |
| `wrapGenerateImages` api match | `RegisteredImagesApiProvider` delegates | `registers_and_resolves_by_api` |
| `wrapGenerateImages` mismatch throws ``Mismatched api: ${model.api} expected ${api}`` | `Err(CallbackError::new(...))` | `rejects_models_with_a_mismatched_api` |

## Gaps
- No differential test: the module is a mutable process-global registry whose only observable behavior is registration/overwrite/lookup plus the wrapper error string; the oracle would mutate shared state without adding coverage beyond the behavioral tests.

## Discrepancies
- None.
