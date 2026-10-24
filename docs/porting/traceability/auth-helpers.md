# Auth helpers — common pin

Frozen `packages/ai/src/auth/helpers.ts`, `4f9e6f8a4` (51 lines). Rust `auth/helpers.rs`.

| Export / branches | Frozen lines | Behavioral proof |
|---|---|---|
| `envApiKeyAuth`, name and login secret prompt/key credential | 11–17 | `auth::env_api_key_login_prompts_for_a_secret` |
| stored credential owns provider; empty/missing key blocks ambient; nonempty key publishes only auth/source | 19–22 | `stored_empty_key_owns_provider_without_ambient_resolution`, `stored_key_does_not_publish_credential_environment`, actual frozen oracle 4 cases |
| no credential: first truthy env; none available | 24–28 | `env_api_key_auth_prefers_stored_key_then_env_order`; frozen oracle |
| `lazyOAuth`: first load shared, rejection retained, login/refresh/toAuth delegated | 39–51 | `lazy_oauth_loads_implementation_once_and_delegates`, `lazy_oauth_retains_rejected_load_for_later_calls`; actual frozen rejection oracle |

RED: 3 new behavioral tests failed (ownership/env leak/load retry). GREEN: existing 16-test auth target plus added 1-test actual-import differential (5 cases). Revision `6cfe4f44` + helper slice; `/tmp/pi-rust-repin-delta-ai/ledger.md` and logs.

## Explicit remaining gaps

Q15c closed the old-only field/check differences: LazyOAuthInput now has only native name/load, API-key login is an optional callback, and helper-local cancellation checks are removed. AuthContext failures propagate through Result; auth14/current provider matrix and raw context/notify source proofs pass. [Current interfaces](auth-types.md).

- A cancelled Rust future during module loading can discard in-flight load; frozen promise persists. Success/rejection reuse proven only for completed loads.

Coverage partial, not new-pin whole-module parity. No source bug identified by current fixtures.
