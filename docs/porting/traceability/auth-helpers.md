# auth helpers traceability

Source: `packages/ai/src/auth/helpers.ts` → `crates/pi-ai/src/auth/helpers.rs`.
Tests: `crates/pi-ai/tests/auth.rs`; differential `auth_resolve`.

## Symbols → proof
| TS | Rust | Test |
|---|---|---|
| `envApiKeyAuth` | `env_api_key_auth` → `EnvApiKeyAuth` | `env_api_key_auth_prefers_stored_key_then_env_order`, `env_api_key_login_prompts_for_a_secret` |
| `lazyOAuth` | `lazy_oauth` → `LazyOAuth`, `LazyOAuthInput`, `OAuthLoader` | `lazy_oauth_loads_implementation_once_and_delegates` |

## Branches
- Stored key wins when non-empty; provider env from the credential is returned with it.
- Env vars are probed in order; empty values are skipped; the winning variable becomes `source`.
- Login checks the flow signal before and after the secret prompt and returns `{type: "api_key", key}`.
- `lazy_oauth` memoizes the loaded implementation behind a Tokio mutex, so concurrent first calls load once.

## Gaps
- TS `load` may reject with any value; Rust uses `AuthError` (`Failed(CallbackError)` or `Aborted`).
