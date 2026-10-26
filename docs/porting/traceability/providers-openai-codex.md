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

## Terra delta (2026-10-26)

Locked delta `5a0ea4d39` adds `gpt-6.1-terra` to the `openai-codex`, `openai-codex-gc`, and `openai-codex-team` catalogs. All three records use the name `GPT-6.1 Terra`, API `openai-codex-responses`, base URL `https://chatgpt.com/backend-api`, reasoning, text and image input, a 922,000-token context window and 128,000 maximum output tokens. Their thinking map is `max → max`, `xhigh → xhigh`, `minimal → low`. Costs per million tokens are $1 input, $5 output, $0.05 cache read and $1.25 cache write; only the provider field differs between records.

`providers_openai_codex::terra_catalog_and_registry_match_locked_typescript` compares each complete native record and registry lookup with the three locked TypeScript shards. `terra_generated_catalog_matches_locked_typescript` compares generated records to those shards. The tests were RED on absent Terra before implementation and GREEN afterward. The artifact manifest records the delta commit and updated payload hashes. Catalog and provider inventory oracles apply the locked delta after checking the common-pin baseline; the generator oracle checks the locked generator delta against the baseline. These integrations do not by themselves establish a new test run.

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

## Common-pin factory closure (2026-10-25)

Scope: entire `ai/src/providers/openai-codex.ts` at 4f9e6f8a4, not the implementation of imported API/OAuth/auth-helper modules. Literal factory fields, catalog/provider rewrites and API dispatch mappings were checked directly against frozen source. Current `differential_providers` passes all six source-import comparisons (factory descriptors/catalog, auth resolution, login callbacks, model-aware auth and request dispatch). The collection differential compares every registered chat/image model value and array order with actual `builtinModels`/`builtinImagesModels`. Google Vertex additionally covers all stored-key branches, empty/missing keys owning ambient auth, explicit key and ADC/project/location cases.

No remaining branch in this factory requires a new carrier or behavior change. Imported-module gaps remain tracked in their own module rows. General owned-return approval applies; no frozen built-in consumer mutates these factory return values through a returned reference. Native API/opaque scalar typed exclusions remain unchanged.
