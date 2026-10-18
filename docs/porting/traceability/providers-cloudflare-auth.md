# providers/cloudflare-auth traceability

Pinned source: `packages/ai/src/providers/cloudflare-auth.ts` at
`532f7d12cd19235081bd4d6e425bde185652d86f`.
Rust: `crates/pi-ai/src/providers/cloudflare_auth.rs`.
Tests: `crates/pi-ai/tests/provider_auth.rs`,
`crates/pi-ai/tests/differential_providers.rs` (`auth_resolution_matches_ts`,
`provider_login_flows_match_ts`).

| TypeScript | Rust | Proof |
|---|---|---|
| `resolveValue(name)` | `resolve_value` | covered through the two auth methods |
| `resolveCloudflareEnv(kind, ...)` | `resolve_cloudflare_env` | `cloudflare_workers_ai_*`, `cloudflare_ai_gateway_*`, differential auth |
| `cloudflareWorkersAIAuth().name` / `login` | `CloudflareWorkersAiAuth` | `cloudflare_login_prompts_for_key_account_and_gateway`, differential login |
| `cloudflareAIGatewayAuth().name` / `login` | `CloudflareAiGatewayAuth` | same |
| workers resolve → `{ auth: { apiKey }, env, source }` | `CloudflareWorkersAiAuth::resolve` | `cloudflare_workers_ai_requires_account_and_returns_scoped_env`, `cloudflare_workers_ai_merges_partial_stored_credential_with_env` |
| gateway resolve → `cf-aig-authorization: Bearer`, `Authorization: null`, `x-api-key: null` | `CloudflareAiGatewayAuth::resolve` | `cloudflare_ai_gateway_requires_gateway_and_suppresses_default_auth_headers` |

## Branch notes

- Per-field merge: credential key/env wins per field; missing fields fall back to
  ambient env. A credential with only the key still picks up the account/gateway
  ids from the environment.
- Workers AI requires key + account; AI Gateway additionally requires the gateway
  id. Missing any required field → `undefined` (no auth).
- `source` is `stored credential` when any credential object was supplied,
  otherwise `CLOUDFLARE_API_KEY`.

## Gaps

- None.

## Discrepancies

- None.
