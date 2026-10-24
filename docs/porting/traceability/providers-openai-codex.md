# providers/openai-codex traceability

Source: `packages/ai/src/providers/openai-codex.ts` at
common pin `4f9e6f8a4` ([source lock](../source-lock.json)).
Rust: `crates/pi-ai/src/providers/openai_codex.rs`.
Tests: `crates/pi-ai/tests/providers_openai_codex.rs` (3 behavioral),
`crates/pi-ai/tests/differential_providers.rs` (provider descriptor + catalog
differential, covering default/GC/Team entries).

| TypeScript | Rust | Proof |
|---|---|---|
| `openaiCodexProvider()` | `openai_codex_provider()` | `default_factory_matches_the_typescript_provider_descriptor`, differential `providers` |
| `openaiCodexProvider(id, name)` | `openai_codex_provider_with_id(id, name)` | `custom_id_rewrites_the_codex_catalog_provider`, differential `providers` |
| `id = "openai-codex"` / `name = "OpenAI Codex"` defaults | same constants | default factory test |
| `baseUrl: "https://chatgpt.com/backend-api"` | same | both factory tests, differential |
| `auth.oauth = lazyOAuth({name: "OpenAI (ChatGPT Plus/Pro)", load: loadOpenAICodexOAuth})` | `ProviderAuth { api_key: None, oauth: Some(lazy_oauth(...)) }` with `load_openai_codex_oauth` | default factory test (name/subscription/login label), differential |
| no `auth.apiKey` | `api_key: None` | default factory test |
| `models: Object.values(OPENAI_CODEX_MODELS).map(m => ({...m, provider: id}))` | `models_data::provider_models("openai-codex")` with `provider = id` | both factory tests (ids, order, rewritten provider, thinkingLevelMap/compat equality) |
| `api: openAICodexResponsesApi()` | `ProviderApi::Single(Arc::new(openai_codex_responses_api()))` | `every_catalog_api_dispatches_to_an_implementation` (codex token + SSE transport) |

## Notes and discrepancies

- The factory always spreads `OPENAI_CODEX_MODELS`, including for the
  `openai-codex-gc` alias; the generated `openai-codex-gc` catalog is imported
  by `providers/all.ts` only through `builtinProviders()`, not through this
  factory. The Rust factory reproduces this (codex catalog values, rewritten
  provider). At the common pin the GC shard equals the rewritten Codex records;
  the former thinking-map inequality is a stale test, replaced with full-record equality.
- TypeScript optional parameters map to two Rust constructors; the
  `openaiCodexProvider(id)` form that omits `name` is a typed exclusion (the
  Rust `_with_id` form always takes a name).
