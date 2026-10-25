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

## Current completion audit

The proposed empty-scoped-path defect was falsified: frozen `getProviderEnvValue` uses `scoped || ambient`, and unchanged Rust matches. The added actual-source real-file/process sequence covers empty-scoped ambient fallback, missing explicit path bypass of a positive cache, and reuse of cached ambient credentials. No production change is required. [Contract](../../specs/env-api-keys.md). Atd6dd70086, current native14 + source differential6 GREEN (suite_core_1). The suite_core_2 probe selects zero tests and is not proof. Required two-package all-target checkPASS; scopedClippy0 errors/8 existing warnings/0 changed-line warnings. Logs: `/tmp/ai-env-current-*`. Changed test lines have no readability findings. Status done under the approved parity bar.

## Explicit runtime gap

Browser-only ADC caching and Node import-readiness timing are not ported (explicit 2026-10-25 parent decision); inherited-prototype lookups and OS/runtime fault shapes follow the common parity bar.

Frozen lines 57–59 now call shared `node_os::homedir()` lazily at the uncached/default-ADC branch. Errors escape as `Result<_, NativeNodeError>` without caching a false result. Explicit credentials and cached probes bypass homedir. `pi-coding-agent/tests/differential_model_selector_nss.rs` forces getpwuid_r EIO through the real registry; exact selector rows/render count match the frozen process. The approved nix-vs-Node NSS errno gap remains explicit in AGENTS.md.
Anthropic provider slice also removed old-only public env-name constants and their shape-only test. Current behavioral count is 14; previous 15 included the deleted constant test.

No frozen source bugs identified in the covered runtime branches.
