# Model catalog generator traceability

Source: `packages/ai/scripts/generate-models.ts` at original runtime pin. No exports; private functions and automatic CLI invocation form the contract. Rust: `crates/pi-ai/src/model_catalog_generator.rs`. Native JSON artifact adaptation approved 2026-10-20.

| Source workflow/symbol | Rust/proof | Status |
|---|---|---|
| `getModelsDevCost` | `get_models_dev_cost`; original TS costs/tiers differential | partial: valid fixtures |
| `processBasetenModels` | `process_baseten_models`; empty/deprecated/plain/toggle/effort/both/GLM fixtures | partial: valid fixtures |
| `processFireworksModels` | `process_fireworks_models`; empty/tool filter/GLM/Kimi/Anthropic fixtures | partial: valid fixtures |
| `recordModelsDevReasoningOptions` | `Generator::record`, lossless key/value storage | partial: recorded by provider transforms; deferred metadata consumer still pending |
| `readGeneratorOptions` | `read_generator_options`; 12 source flag/argument/error cases | implemented; OS cwd failure proof open |
| automatic CLI | pending native entrypoint | gap |
| OpenRouter/AI Gateway HTTP | `Generator::fetch_open_router_models/fetch_ai_gateway_models`; tool/image/reasoning/pricing/raw UTF-16/default/strict 503 fixtures | partial: malformed shape/log/transport coverage remains |
| NVIDIA HTTP | `fetch_nvidia_nim_model_ids`; exercised through all-provider pipeline fixture | partial: normalized alias/duplicate/error proofs pending |
| `loadModelsDevData` (all source provider families) | `models_dev.rs`; all-provider fixture including family exclusions, routing, aliases, subscription/implied prices and individual Qwen allowlist | partial: alias precedence/raw errors/strict/normalization counterexamples still open |
| compat/thinking/strict-tools/grammar/tool-search/cache metadata helpers | `metadata.rs`, `apply_model_metadata`; 675 provider/id/API cases with existing compatibility/thinking fields, 4050 original TS calls | partial: full Cartesian/base-URL/default/invalid-type branch proofs pending; deferred models.dev reasoning records not yet wired |
| hardcoded supplements, overrides, Azure clones | pending | gap |
| priority/dedup/sorted provider/API/model catalogs | pending | gap |
| native artifacts/manifest/hash/staging/validation/rollback/stats | pending shared model-data integration | gap |

## Proof

`cargo test -p pi-ai --test model_catalog_generator -- --nocapture`: RED expected null vs source cost object; GREEN 1 test with 9 original-source requests. Logs: `/tmp/pi-rust-model-generator-proof/02-red.log`, `03-green.log`. Oracle transpiles actual pinned source with TypeScript, removes automatic invocation via AST and exposes unchanged private functions. Process group/watchdog uses existing `support/oracle.rs`. No network refresh or source writes.

Focused follow-up logs: `05-http-red.log`/`06-options-red.log` establish missing HTTP/options behavior; `08-targeted-red.log` proves HTTP/options GREEN and metadata/models.dev/raw errors RED after shared dependencies. `09-raw-green.log` proves source cost/provider regression and eight raw UTF-16/error cases GREEN. Shared `is_truthy`/fallible `to_js_string` and binary64 `JsValue::Number` are reused; no local coercion duplicate remains.

`12-metadata-green.log`: 675 provider/model/API combinations compare merged model bytes against six sequential original-source mutation functions (4050 source calls), GREEN; no-op RED recorded in `08-targeted-red.log`. This does not establish exhaustive branch coverage or the deferred reasoning-record pass.

`14-models-dev.log`: all-source-provider fixture GREEN against actual `loadModelsDevData`, comparing every resulting model's bytes and sequence, plus exact models.dev→NVIDIA request order. RED empty pipeline was recorded in `08-targeted-red.log`. This proves the fixture, not exhaustive raw payloads or final generator workflow.

## Pending whole-module branch inventory

- HTTP: NVIDIA direct/normalized ID map and duplicate overwrite; conditional NVIDIA lookup only when models.dev has a truthy NVIDIA models collection; status/transport/JSON/shape failures; strict rethrow versus source default empty results; exact fetch/log order.
- Provider transforms: Bedrock exclusions/region/strict flag; Anthropic; Google/Vertex alias metadata and Vertex Gemini allowlist/cache correction; OpenAI unsupported alias; Groq/Cerebras; both Cloudflare paths; xAI Responses selection; ZAI coding variants/stream exclusions/GLM-5.2; Mistral cache default; HuggingFace; NVIDIA modality/live allowlist/exclusions; Together aliases/deprecation/reasoning modes; OpenCode/Go npm API routing and native endpoint corrections; Copilot family routing/headers/tier costs; MiniMax variants; Kimi aliases/implied rates; Moonshot K3; Xiaomi variants; Qwen regional/individual filtering and strict exact-ID assertion.
- Metadata helpers: `mergeThinkingLevelMap`, `getModelKey`, `recordModelsDevReasoningOptions`, `supportsDirectReasoningEffort`, `applyModelsDevReasoningOptionMetadata`, Together compatibility/maps, OpenAI xhigh/max detection, Google/Anthropic family predicates, compatibility detection/default delta/merges, strict tools, grammar tools, tool search, explicit cache, complete thinking-level branches, Anthropic provider compatibility, Bedrock region, NVIDIA normalization and six-digit rounding.
- Orchestration: sequential three-source loading; exclusions; Copilot/Anthropic/OpenCode/OpenAI/gateway/Kimi pricing/context corrections; missing OpenAI/DeepSeek/Ant-Ling supplements; minimax allowlist; Codex/GC lists; Mistral/OpenRouter aliases; Azure cloning; metadata order; first-source-wins dedup; UTF-16 lexical provider/model/API sorting; grouped private versus flat public catalogs; data-only missing-provider guard; native include-table adaptation; generated timestamp; staged manifest/validation and transactional restore; stale shard cleanup; public JSON output removal/recreation; pre-dedup model statistics and provider insertion-order statistics; CLI error exit.

## Approved error-output adaptation

Alessio, 2026-10-20: “Omit JS frames (Recommended)”. Native failures preserve error names/messages, log order and exit behavior while omitting JavaScript-specific stack frames. Oracle responses retain full source stack diagnostics separately; this is not permission to suppress errors.

## Representation and exclusions

Raw provider values use shared `JsValue`/`JsString`, preserving UTF-16 code units and JS object enumeration. Existing typed reasoning converter is reused, not cloned; unknown efforts cannot produce a supported Pi effort. No runtime-model migration. Raw tests additionally prove distinct lone-surrogate IDs/names and errors for malformed cost tiers, null models, malformed Baseten reasoning options and malformed Fireworks modalities. Other malformed-payload branches remain open; do not treat these fixtures as whole-module parity.
