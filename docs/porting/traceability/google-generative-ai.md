# google-generative-ai traceability

Source: `packages/ai/src/api/google-generative-ai.ts` and `google-generative-ai.lazy.ts`.
Rust: `crates/pi-ai/src/api/google_generative_ai.rs` (plus the private SDK stand-in `google_client.rs`).
Tests: `crates/pi-ai/tests/differential_google_api.rs` (45 recorded-fixture scenarios, ids 0-44, against the pinned TypeScript module), `crates/pi-ai/tests/google_api.rs` (8 behavioral), `crates/pi-ai/tests/google_shared.rs`.

## Representation
- `GoogleOptions extends StreamOptions` → `GoogleOptions { stream, tool_choice, thinking }`, parsed from `StreamOptions::extra` by `GoogleOptions::from_stream_options`. Unknown extra keys are ignored like JavaScript property access.
- `GoogleThinkingLevel` is the shared enum in `google_shared.rs` with the exact Google wire values.
- `stream`/`streamSimple` return `AssistantMessageEventStream`; the TypeScript async IIFE becomes a spawned task pushing into the same event stream. Synchronous TypeScript throws (`streamSimple` without an API key) become an immediate `error` event, the only representable shape of the Rust stream contract.
- `Date.now()` tool-call ids use a process-global `AtomicU64` counter; the differential oracle normalizes generated ids (`<generated>`).
- Request bodies are `@google/genai`'s wire transformation of `GenerateContentParameters`: `google_client::request_body` reproduces field whitelists, ordering, and per-API differences (Gemini `functionCall` reordering and `inlineData` `data`-first, Vertex pass-through).

## Symbols → proof
| TS | Rust | Test |
|---|---|---|
| `GoogleOptions` | `GoogleOptions` | differential ids 0, 14-19 |
| `stream` | `stream` | differential ids 0, 14-44 |
| `streamSimple` | `stream_simple` | differential ids 1-13, `gemini_stream_simple_without_an_api_key_returns_an_error_stream` |
| `createClient` | `GoogleClient::generative_ai` | differential request URLs/headers |
| `buildParams` | `build_params` | differential request bodies |
| thinking enabled/level/budget | `thinking_config`, `get_thinking_level`, `get_google_budget` | differential ids 2-15 |
| disabled thinking per model | `get_disabled_thinking_config` | differential ids 16-19 |
| `isGemini3ProModel` / `isGemini3FlashModel` / `isGemma4Model` | `is_gemini3_pro_model`, `is_gemini3_flash_model`, `is_gemma4_model` | differential ids 9-12, 17-19 |
| `toolCallCounter` ids | `generated_tool_call_id` | differential ids 21, 22 |
| `googleGenerativeAIApi` (lazy) | `google_generative_ai_api` | `lazy_api_wrappers_expose_the_provider_streams_contract` |

## Branch notes
- `start` is pushed only after the request resolves (headers), matching TypeScript.
- Text/thinking blocks are opened and closed by part kind; signatures are retained per block via `retain_thought_signature`.
- `functionCall` closes the current text/thinking block; missing or duplicate ids get `${name}_${Date.now()}_${counter}`.
- Any tool call in the message overrides the provider finish reason with `toolUse`.
- Usage metadata replaces the whole usage record per chunk (`input = prompt - cached`, `output = candidates + thoughts`), then `calculateCost` runs.
- Stop reasons: pending → error, aborted/error → `Provider stopped with: <raw>`; unknown finish reasons throw `Unhandled stop reason: <reason>`.
- `onPayload` receives the `GenerateContentParameters` object before the SDK wire transform and may replace it.

## Gaps
- `@google/genai`'s `GOOGLE_GEMINI_BASE_URL` process-env fallback is implemented in `GoogleClient::generative_ai` but not exercised differentially (setting process env per scenario is not isolated).
- The TypeScript adapter rejects `options.fetch !== globalThis.fetch`; Rust always routes through `options.request.fetch` or `http::default_fetch()` per the port's `Fetch` contract (approved deviation, required by the differential harness).
- `part.functionCall.args` that is not a JSON object is excluded by the typed core (`{}` is used); TypeScript keeps the invalid value.
- A non-object `onPayload` replacement makes the SDK wire transform throw; the Rust typed core reports the transform error instead.

## Discrepancies
- None in `google-generative-ai.ts` itself. The `streamSimple` project/location drop is a Vertex-module discrepancy (see `google-vertex.md`).
