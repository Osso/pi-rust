# auth context traceability

Source: `packages/ai/src/auth/context.ts` → `crates/pi-ai/src/auth/context.rs`.

## Symbols → proof
| TS | Rust | Proof |
|---|---|---|
| `defaultProviderAuthContext()` | `default_provider_auth_context()` / `DefaultProviderAuthContext` | exercised through `create_models` defaults and provider slices |
| `env(name)` | `AuthContext::env` | Reads shared process_env; native Result can preserve injected callback failures. Default process-env behavior still needs a dedicated current-pin matrix. |
| `fileExists(path)` | `AuthContext::file_exists` | implementation expands a leading `~` with `HOME` and checks metadata |

## Gaps
- Default process-env trimming and home expansion still need isolated current-pin differential tests; injected-context proof is not a whole-default-context proof. Shared process_env and child-process fixtures permit safe isolation.
- `node:fs` async access → `std::fs::metadata` inside the async function. This is a blocking filesystem probe, matching the TS contract (existence only) without adding a Tokio `fs` feature.
- Browser behavior (`fileExists` always false) is not applicable to the Rust core.

## Q18 native auth context

AuthContext env values and filesystem path arguments are `JsString`; OS lookup widens scalar values and filesystem existence encodes only at metadata. `lossless_auth_interaction::auth_context_encodes_raw_path_only_at_filesystem_lookup` now passes. Overlay env retains native ProviderEnv values. Q15c introduces native Result channels for env/file_exists; 21 actual-provider rejection cases (including abort(undefined) default) preserve value, branding and call order. [Current type proof](auth-types.md).
