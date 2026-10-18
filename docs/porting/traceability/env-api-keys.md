# env-api-keys traceability

Source: `packages/ai/src/env-api-keys.ts`.
Rust: `crates/pi-ai/src/env_api_keys.rs`.
Tests: `crates/pi-ai/tests/env_api_keys.rs` (14 behavioral), `crates/pi-ai/tests/differential_env_api_keys.rs` (5 tests; 167 provider/env requests plus Vertex ADC and Bedrock ambient matrices vs pinned TS).

## Representation
- `findEnvKeys(provider, env?)` → `find_env_keys(provider, Option<&ProviderEnv>)`; testable form `find_env_keys_with(provider, env, process_env)`.
- `getEnvApiKey(provider, env?)` → `get_env_api_key` / `get_env_api_key_with`.
- `ProviderEnv` is the existing `BTreeMap<String, String>`; scoped values win over process values and empty values fall through (`||`), reusing `utils::provider_env::resolve_provider_env_value`.
- `hasVertexAdcCredentials` → `VertexAdc`, which carries the injectable file-existence probe, home directory, and the module-level cache. `VertexAdc::default_sources()` uses `std::fs::metadata` and `$HOME`.
- `getApiKeyEnvVars` returns `Option<Vec<&'static str>>`; the TypeScript map is a `match`.

## Symbols → proof
| TS | Rust | Test |
|---|---|---|
| `ANTHROPIC_AUTH_TOKEN_ENV` / `ANTHROPIC_OAUTH_TOKEN_ENV` / `ANTHROPIC_API_KEY_ENV` | same constants | `anthropic_env_names_are_pinned` |
| `getApiKeyEnvVars` (`github-copilot`, `anthropic` special cases + map) | `api_key_env_vars` | `api_key_env_map_covers_known_providers`, `copilot_credentials_come_from_copilot_github_token`, differential matrix |
| `findEnvKeys` filters configured names, `undefined` when none | `find_env_keys_with` | `generic_github_tokens_are_not_copilot_credentials`, `empty_values_do_not_count_as_configured`, differential |
| `getEnvApiKey` anthropic skips `ANTHROPIC_AUTH_TOKEN` | `get_env_api_key_with` | `anthropic_reports_auth_token_but_never_returns_it_as_api_key`, `anthropic_falls_back_to_oauth_then_api_key`, differential |
| scoped env precedence (`getProviderEnvValue`) | `resolve_provider_env_value` | `scoped_env_wins_over_process_env`, differential |
| `hasVertexAdcCredentials` explicit scoped path | `VertexAdc::has_credentials` first branch | `vertex_explicit_credentials_path_is_checked_directly`, `vertex_adc_result_is_cached_but_explicit_paths_bypass_the_cache`, differential |
| `hasVertexAdcCredentials` process env path, default ADC path, cache | `VertexAdc::has_credentials` | `vertex_adc_result_is_cached_but_explicit_paths_bypass_the_cache`, `vertex_requires_adc_credentials_project_and_location`, differential |
| Vertex requires project + location | `has_vertex_credentials` | `vertex_requires_adc_credentials_project_and_location`, differential |
| Bedrock ambient sources (six) | `has_bedrock_credentials` | `bedrock_ambient_credential_sources_report_authenticated`, differential |
| unknown provider → `undefined` | `None` | `unknown_providers_have_no_env_keys`, differential |

## Gaps
- `_existsSync`/`_homedir`/`_join` lazy dynamic imports and the "Node modules not loaded yet" retry branch exist only to avoid breaking browser/Vite builds and to work around asynchronous import startup. Native Rust always has synchronous filesystem access, so the module resolves those sources eagerly through `VertexAdc::default_sources()`. The browser-specific "cache false permanently" branch is unreachable in Rust.
- `homedir()` uses the passwd database when `$HOME` is unset in Node; the Rust port reads `$HOME` only. Documented in `VertexAdc::default_sources`; no fallback is added.

## Discrepancies
- None.
