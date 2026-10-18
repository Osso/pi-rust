# providers/anthropic traceability

Pinned source: `packages/ai/src/providers/anthropic.ts` at
`532f7d12cd19235081bd4d6e425bde185652d86f`.
Rust: `crates/pi-ai/src/providers/anthropic.rs`.
Tests: `crates/pi-ai/tests/provider_auth.rs`,
`crates/pi-ai/tests/differential_providers.rs` (`auth_resolution_matches_ts`,
`provider_login_flows_match_ts`, `provider_dispatch_matches_ts_requests`).

| TypeScript | Rust | Proof |
|---|---|---|
| `anthropicApiKeyAuth().name` | `AnthropicApiKeyAuth::name` (`"Anthropic API key"`) | `provider_metadata_matches_the_ts_factories` |
| `login`: abort check, secret prompt, abort check | `AnthropicApiKeyAuth::login` | `anthropic_login_prompts_for_a_secret`, differential login |
| `resolve`: abort → stored key → `ANTHROPIC_AUTH_TOKEN` bearer → `ANTHROPIC_OAUTH_TOKEN` → `ANTHROPIC_API_KEY` → `undefined` | `AnthropicApiKeyAuth::resolve` | `anthropic_resolves_auth_token_as_bearer_header`, `anthropic_prefers_oauth_token_over_api_key`, `anthropic_api_key_is_last_and_stored_credential_wins`, `anthropic_resolve_honors_abort_before_env_reads`, differential auth |
| `anthropicProvider()` metadata/catalog/api | `anthropic_provider` | `provider_metadata_matches_the_ts_factories`, dispatch differential |
| `lazyOAuth({ name, isSubscription: true, load: loadAnthropicOAuth })` | `lazy_oauth` with `load_anthropic_oauth` | `oauth_subscription_flags_match_the_ts_factories`, `oauth_load.rs` |

## Branch notes

- The bearer branch returns `headers.Authorization = "Bearer <token>"` and no
  `apiKey`; the OAuth/API-key branches return `apiKey` only.
- Stored credentials win over every env var and carry `credential.env` through.
- Abort is checked before the first env read, after each `ctx.env` call, and
  before returning; a cancelled signal yields `AuthError::Aborted`.

## Gaps

- None.

## Discrepancies

- None.
