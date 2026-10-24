# providers/* factories traceability

Pinned source: `packages/ai/src/providers/<name>.ts` provider factories at
`532f7d12cd19235081bd4d6e425bde185652d86f`.
Historical evidence below uses the old pin; it is not whole-factory parity evidence for the common pin.
Rust: `crates/pi-ai/src/providers/<name>.rs` (one module per retained TypeScript file).
Tests: `crates/pi-ai/tests/providers.rs`,
`crates/pi-ai/tests/differential_providers.rs`.

Every factory is `createProvider({ id, name, baseUrl?, auth, models, api })`.
Rust uses `providers::env_key_provider` (catalog + `envApiKeyAuth`) or
`providers::custom_provider` (custom auth/filter) and builds `ProviderApi`
directly.

## OpenCode Go common-pin revalidation

Frozen opencode-go.ts:7–17 names OpenCode Zen Go and maps only Anthropic/Completions. At6c1454e8 descriptor RED proves old name; e34a95ed public-provider differential RED proves native Responses sends an HTTP request while frozen returns an unsupported-API error without one. Native removes that map entry/import and uses frozen name. Complete transcript GREEN remains to record.

## Current factory-field revalidation

Actual-source a12f44de descriptor/picker comparison is RED before production changes. Common-pin github-copilot.ts:10–26 has no subscription flag/filter; openai-codex.ts:17 has no subscription flag; openrouter.ts:6–14 has API-key auth only, no OAuth. Native removes these old-only fields rather than retaining compatibility paths. Complete factory descriptor/picker and dispatch GREEN remains to record.

## Removed old-only factories

Common pin: `4f9e6f8a4` ([source lock](../source-lock.json)). Frozen
`packages/ai/src/providers/` has none of these six modules; `all.ts`
imports/calls none, and `index.ts` exports no such factory. Rust modules and
`providers/mod.rs` exports were deleted, not replaced with fallbacks.

| Absent frozen module | Removed Rust public symbols |
|---|---|
| `azure-openai-responses.ts` | `azure_openai_responses_provider` |
| `baseten.ts` | `baseten_provider` |
| `qwen-token-plan.ts` | `qwen_token_plan_provider` |
| `qwen-token-plan-cn.ts` | `qwen_token_plan_cn_provider` |
| `qwen-token-plan-individual.ts` | `qwen_token_plan_individual_provider` |
| `radius.ts` | `RadiusProviderOptions`, `RadiusProvider`, `radius_provider`, `radius_provider_with_fetch` |

Frozen `api/azure-openai-responses.ts` / `.lazy.ts` and root
`AzureOpenAIResponsesOptions` type export still exist: Rust API, compat
and legacy aliases remain. Baseten/Qwen/Radius have no frozen API modules;
shared OpenAI APIs remain. Unrelated old-only Radius auth/config/pi-messages
and model-generator contracts remain staged; this removal does not certify them.

Consumer reconciliation: delete Azure-positive and Radius-factory tests;
remove absent metadata entries and add Team to OAuth fixture, derived from
frozen `all.ts:80-119`, `openai-codex.ts:7-20` and `auth/helpers.ts:39-52`.
Coding-agent no longer synthesizes Radius factories from config; builtin
reset remains, without changing staged timestamp/remote-catalog wiring.

Proof boundary: baseline RED reproduces Azure's missing-catalog panic and
stale metadata/OAuth fixtures. Focused providers/catalog/inventory tests cover
retained inventory and behavior; workspace all-target checking covers in-repo
consumer compilation without removed exports. Neither establishes whole-provider
protocol parity; see [catalog traceability](providers-models.md).

## Plain `envApiKeyAuth` factories

`ant-ling`, `cerebras`, `deepseek`, `groq`, `huggingface`,
`minimax`, `minimax-cn`, `mistral`, `moonshotai`, `moonshotai-cn`, `nvidia`,
`openai`, `together`, `vercel-ai-gateway`, `xiaomi`,
`xiaomi-token-plan-ams`, `xiaomi-token-plan-cn`, `xiaomi-token-plan-sgp`,
`zai`, `zai-coding-cn`, `google`.

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
- Azure retains only its API implementation/type export at the common pin;
  the old-only factory/catalog and factory-positive test are removed.
- `mistral` is the only `mistral-conversations` provider; `google` is the only
  `google-generative-ai` provider.

## Gaps

- Whole-factory/auth/protocol parity must be revalidated against the common
  pin. Frozen builtin inventory includes default/GC/Team Codex; actual-source
  inventory differential covers IDs/order, not whole-factory semantics.

## Discrepancies

- `opencode-go.models.ts` contains `gemini-4-argon-contributor` with api
  `google-generative-ai`, but `opencode-go.ts` declares no implementation for
  that api. Dispatching it yields `Provider opencode-go has no API implementation
  for "google-generative-ai"` in both TS and Rust. Rust preserves the source
  behavior (`every_catalog_api_dispatches_to_an_implementation`,
  `provider_dispatch_matches_ts_requests`).
- `providers.test.ts` asserts `gpt-4.1` exists in the Copilot catalog; the pinned
  catalog has no such id. Rust tests use catalog ids (`gpt-5.4`).
