# provider-env traceability

Source: `packages/ai/src/utils/provider-env.ts`.
Rust: `crates/pi-ai/src/utils/provider_env.rs`.
Tests: `crates/pi-ai/tests/provider_env.rs` (4 behavioral), `crates/pi-ai/tests/differential_provider_env.rs` (6 requests vs pinned TS).

## Representation
- `getProviderEnvValue(name, env)` → `get_provider_env_value(name, Option<&ProviderEnv>)`.
- Process environment is read with `std::env::var`.
- `resolve_provider_env_value(name, env, process_env)` is the injectable form used by tests; it mirrors the `||` precedence without touching process-global state.

## Symbols → proof
| TS | Rust | Test |
|---|---|---|
| `getProviderEnvValue` | `get_provider_env_value` / `resolve_provider_env_value` | `reads_the_real_process_environment_when_unscoped`, differential |
| scoped override wins | `resolve_provider_env_value` | `scoped_override_wins_over_process_env` |
| empty override falls through (`\|\|` truthiness) | `.filter(!is_empty)` | `empty_scoped_override_falls_through_to_process_env` |
| missing everywhere → `undefined` | `None` | `missing_everywhere_is_none` |
| `getBunSandboxEnvValue` (`/proc/self/environ` when Bun `process.env` is empty) | not ported | — |

## Gaps
- The `/proc/self/environ` fallback is a Bun-compiled-binary workaround for an empty `process.env`. Native Rust reads its own environment through `std::env`, so the trigger condition (Bun runtime + empty environment object) cannot occur. The module-level env cache and `node:fs` read are therefore intentionally omitted rather than emulated.

## Discrepancies
- None.
