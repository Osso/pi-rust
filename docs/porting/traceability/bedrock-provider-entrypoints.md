# Bedrock package forwarding entrypoints

Original [pinned](../source-lock.json) modules:

| Source | Complete exported contract | Rust public surface | Status |
|---|---|---|---|
| `packages/ai/bedrock-provider.js` | `export * from "./dist/bedrock-provider.js";` | `pi_ai::bedrock_provider::bedrock_provider_module` | done |
| `packages/ai/bedrock-provider.d.ts` | Same one-line forwarding declaration | Same public typed API | done |

Both files contain only that export; neither has independent branching, wrapper logic, initialization or additional exports. The target source `packages/ai/src/bedrock-provider.ts` exports `bedrockProviderModule = { stream, streamSimple }`. Rust `crates/pi-ai/src/lib.rs:6` publicly exposes `bedrock_provider`; its `bedrock_provider_module()` returns `BedrockProviderModule`. `api/bedrock_converse_stream.rs:238–250` implements both `ProviderStreams::stream` and `stream_simple` by directly forwarding to the concrete stream functions. This is native public API equivalence, not JS package-path compatibility or Node/browser bundling execution.

Concrete external consumer: `crates/pi-ai/tests/bedrock_provider.rs` imports that public module and `ProviderStreams`, streams a scripted response through `bedrock_provider_module().stream(...)`, asserts text `Hello` and stop reason, then verifies the lazy override. Existing test `provider_module_and_lazy_override_match_the_typescript_exports` passed in the current independent log; the underlying simple-stream behavior also has `stream_simple_adjusts_thinking_budget`. Exact pass locations and revision: [proof ledger](../proven-assets-proof.md). The forwarding-only modules add no behavior beyond that existing API; no new consumer/build/test run or TS declaration-loader parity is claimed.

Target module/override mapping remains in [bedrock-provider](bedrock-provider.md); stream branches remain in [Bedrock API traceability](bedrock-converse-stream.md). No new exclusion.
