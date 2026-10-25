# legacy-api-aliases traceability

Source: `packages/ai/src/legacy-api-aliases.ts` at
`532f7d12cd19235081bd4d6e425bde185652d86f`.
Rust: `crates/pi-ai/src/legacy_api_aliases.rs`.
Tests: `crates/pi-ai/tests/differential_legacy_api_aliases.rs` (16 alias calls
against the pinned module: request URL/method, stop reason, error message).

## Symbols → proof

Every TypeScript alias is a bare reference to `*Api().stream`/`streamSimple`.
Rust exposes one function per alias that constructs the same lazy api wrapper
and forwards with the same `StreamOptions`/`SimpleStreamOptions`.

| TypeScript | Rust |
|---|---|
| `streamAnthropic` / `streamSimpleAnthropic` | `stream_anthropic` / `stream_simple_anthropic` |
| `streamAzureOpenAIResponses` / `streamSimpleAzureOpenAIResponses` | `stream_azure_openai_responses` / `stream_simple_azure_openai_responses` |
| `streamGoogle` / `streamSimpleGoogle` | `stream_google` / `stream_simple_google` |
| `streamGoogleVertex` / `streamSimpleGoogleVertex` | `stream_google_vertex` / `stream_simple_google_vertex` |
| `streamMistral` / `streamSimpleMistral` | `stream_mistral` / `stream_simple_mistral` |
| `streamOpenAICodexResponses` / `streamSimpleOpenAICodexResponses` | `stream_openai_codex_responses` / `stream_simple_openai_codex_responses` |
| `streamOpenAICompletions` / `streamSimpleOpenAICompletions` | `stream_openai_completions` / `stream_simple_openai_completions` |
| `streamOpenAIResponses` / `streamSimpleOpenAIResponses` | `stream_openai_responses` / `stream_simple_openai_responses` |

Differential cases per api: a model with the API's `api`/provider/base URL, a
scripted 400 response, and both aliases. Each case asserts the request URL and
method, `stopReason`, and `errorMessage`. Codex uses a JWT access token and
`transport: sse`; Google/Vertex use the global fetch replacement in TypeScript
(their SDK adapters reject `options.fetch`), while Rust injects the recording
fetch.

## Required acquisition propagation (2026-10-24)

Common-pin legacy-api-aliases.ts:19–28,30–108 aliases the lazy factory methods without a catch. Native aliases therefore forward the shared acquisition Result directly; they do not encode a registered rejection as an event. LazyApi itself returns Ok(outer stream), with inner acquisition owned by the lazy setup catch. Existing alias behavior tests need renewed proof after this signature migration; the old-pin coverage below is not current acceptance.

## Notes

- TypeScript constructs the lazy api instances once at module load; Rust
  constructs a fresh stateless `LazyApi` per call. `LazyApi` carries no mutable
  state, so the observable behavior is identical.
- Typed per-API options (`AnthropicOptions`, ...) are not parameters here; they
  live in `StreamOptions::extra` and are parsed by each API module (see
  `ai-types.md`).

## Gaps

- None. The underlying API modules have their own differential coverage; this
  module only proves the delegation wiring.
