# auth context traceability

Source: `packages/ai/src/auth/context.ts` → `crates/pi-ai/src/auth/context.rs`.

## Symbols → proof
| TS | Rust | Proof |
|---|---|---|
| `defaultProviderAuthContext()` | `default_provider_auth_context()` / `DefaultProviderAuthContext` | exercised through `create_models` defaults and provider slices |
| `env(name)` | `AuthContext::env` | implementation reads `std::env`, returning `None` for missing or whitespace-only values (matches TS truthiness check) |
| `fileExists(path)` | `AuthContext::file_exists` | implementation expands a leading `~` with `HOME` and checks metadata |

## Gaps
- No dedicated unit test for the process-env branch: mutating process env is unsafe/racy in parallel Rust tests. The trim/empty rule mirrors the TS condition literally; injectable contexts are tested in `tests/auth.rs` and `tests/models.rs`.
- `node:fs` async access → `std::fs::metadata` inside the async function. This is a blocking filesystem probe, matching the TS contract (existence only) without adding a Tokio `fs` feature.
- Browser behavior (`fileExists` always false) is not applicable to the Rust core.
