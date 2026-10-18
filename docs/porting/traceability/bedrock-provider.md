# `ai/src/bedrock-provider.ts` + `ai/src/api/bedrock-converse-stream.lazy.ts` traceability

Source: `packages/ai/src/bedrock-provider.ts`,
`packages/ai/src/api/bedrock-converse-stream.lazy.ts`.
Rust: `crates/pi-ai/src/bedrock_provider.rs`.

| TypeScript | Rust | Tests |
| --- | --- | --- |
| `bedrockProviderModule` (`{ stream, streamSimple }`) | `bedrock_provider_module()` returning `BedrockProviderModule` (`ProviderStreams`) | `crates/pi-ai/tests/bedrock_provider.rs` |
| `setBedrockProviderModule(module)` | `set_bedrock_provider_module(Arc<dyn ProviderStreams>)` | override branch in `bedrock_provider.rs` test |
| `bedrockConverseStreamApi()` (`lazyApi`) | `bedrock_converse_stream_api()` returning `LazyApi` | lazy override branch in `bedrock_provider.rs` test |

The TypeScript variable-specifier dynamic import exists only to keep the
Node-only AWS SDK out of browser bundles. Rust links the implementation
directly; the lazy loader and override seam are preserved for callers that
replace the module (the Bun build use case). No browser branch exists in Rust.

Gaps: none observable; the lazy loader is synchronous-resolution wrapped in the
shared `lazy_stream` machinery.
