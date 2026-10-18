# providers/* factories traceability

Pinned source: `packages/ai/src/providers/<name>.ts` provider factories at
`532f7d12cd19235081bd4d6e425bde185652d86f`.
Rust: `crates/pi-ai/src/providers/<name>.rs` (one module per TypeScript file).
Tests: `crates/pi-ai/tests/providers.rs`,
`crates/pi-ai/tests/differential_providers.rs`.

Every factory is `createProvider({ id, name, baseUrl?, auth, models, api })`.
Rust uses `providers::env_key_provider` (catalog + `envApiKeyAuth`) or
`providers::custom_provider` (custom auth/filter) and builds `ProviderApi`
directly.

## Plain `envApiKeyAuth` factories

`ant-ling`, `baseten`, `cerebras`, `deepseek`, `groq`, `huggingface`,
`minimax`, `minimax-cn`, `mistral`, `moonshotai`, `moonshotai-cn`, `nvidia`,
`openai`, `qwen-token-plan`, `qwen-token-plan-cn`,
`qwen-token-plan-individual`, `together`, `vercel-ai-gateway`, `xiaomi`,
`xiaomi-token-plan-ams`, `xiaomi-token-plan-cn`, `xiaomi-token-plan-sgp`,
`zai`, `zai-coding-cn`, `google`, `azure-openai-responses`.

| Contract | Rust | Proof |
|---|---|---|
| id/name/baseUrl/auth name/env vars | `ProviderSpec` in each module | `provider_metadata_matches_the_ts_factories`, differential provider descriptors |
| model list from the generated catalog | `provider_models(id)` | `builtin_models_registers_every_builtin_provider_with_models`, differential |
| single API implementation | `ProviderApi::Single(Arc::new(<api>()))` | `every_catalog_api_dispatches_to_an_implementation`, differential request dispatch |
| api-key resolution (stored key → env order) | `auth::helpers::env_api_key_auth` | `tests/auth.rs`, differential auth cases |

## Multi-API factories

| Module | APIs | Proof |
|---|---|---|
| `fireworks` | anthropic-messages, openai-completions | dispatch differential |
| `opencode` | anthropic-messages, google-generative-ai, openai-completions, openai-responses | dispatch differential |
| `opencode-go` | anthropic-messages, openai-completions, openai-responses | dispatch differential |
| `xai` | openai-completions, openai-responses (+ lazy xAI OAuth) | dispatch + auth differential |
| `github-copilot` | anthropic-messages, openai-completions, openai-responses (+ OAuth, filter) | `providers-github-copilot.md` |

## Branch notes

- `Object.values(<PROVIDER>_MODELS)` order is preserved; models are stored in
  catalog order (`models_data`).
- `azure-openai-responses` has a catalog and factory but is **not** part of
  `builtinProviders`; `tests/providers.rs` asserts both facts.
- `mistral` is the only `mistral-conversations` provider; `google` is the only
  `google-generative-ai` provider.

## Gaps

- `openai-codex.ts` (and `openai-codex-gc`) belong to the in-flight api-codex
  slice; `builtin_providers()` omits them and the differential asserts exactly
  those two ids are missing.

## Discrepancies

- `opencode-go.models.ts` contains `gemini-4-argon-contributor` with api
  `google-generative-ai`, but `opencode-go.ts` declares no implementation for
  that api. Dispatching it yields `Provider opencode-go has no API implementation
  for "google-generative-ai"` in both TS and Rust. Rust preserves the source
  behavior (`every_catalog_api_dispatches_to_an_implementation`,
  `provider_dispatch_matches_ts_requests`).
- `providers.test.ts` asserts `gpt-4.1` exists in the Copilot catalog; the pinned
  catalog has no such id. Rust tests use catalog ids (`gpt-5.4`).
