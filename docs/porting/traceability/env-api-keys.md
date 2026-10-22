# env-api-keys — common-pin re-port

Frozen: `packages/ai/src/env-api-keys.ts`, `4f9e6f8a4` (176 lines).
Rust: `crates/pi-ai/src/env_api_keys.rs`.

| Frozen symbols / branches | Rust / behavioral proof |
|---|---|
| `getApiKeyEnvVars`, lines 64–113: Copilot, OAuth-before-API Anthropic, complete provider map, unknown | `api_key_env_vars`; `env_api_keys::api_key_env_map_covers_known_providers`, `removed_provider_keys_are_not_ambient_credentials`; `differential_env_api_keys` provider/env matrix |
| `findEnvKeys`, lines 123–131: configured values only; no ambient sources | `find_env_keys_with`; empty/scoped/unknown/Copilot tests; differential |
| `getEnvApiKey`, lines 138–143: first configured key | `get_env_api_key_with`; `anthropic_ignores_auth_token_and_prefers_oauth`, `anthropic_falls_back_to_oauth_then_api_key`; differential |
| ADC scoped path/cache/process path/default path, lines 30–61; project/location, 147–158 | `VertexAdc`, `has_vertex_credentials`; explicit path/cache/project tests and differential ADC fixtures |
| Bedrock six ambient sources, lines 161–173 | `has_bedrock_credentials`; behavioral six-source/partial-key matrix + differential |

Removed old-only Qwen/Radius/Baseten environment mappings and Anthropic auth-token lookup. Frozen map has neither; RED tests retained negative assertions. API values remain `JsString`; filesystem encoding uses Node UTF-8 at filesystem boundary.

## Current proof

`50307512` + env slice: `cargo test -j4 -p pi-ai --test env_api_keys` — 15 GREEN; `--test differential_env_api_keys` — 5 GREEN. RED: 2 failures before implementation. Logs and revision ledger: `/tmp/pi-rust-repin-delta-ai/`.

## Explicit gaps — partial, not approved new-pin parity

- Frozen lines 1–26, 39–48: asynchronous Node import readiness/browser caching absent in native Rust.
- Frozen lines 57–59: Node `homedir()` uses passwd when HOME absent; Rust default probe uses HOME only. No fallback added.
- Rust still exposes three env-name constants used by provider auth; frozen exports only the two functions. Anthropic provider slice will remove its old-only constant dependency.

No frozen source bugs identified in the covered runtime matrix.
