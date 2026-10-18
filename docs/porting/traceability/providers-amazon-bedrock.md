# providers/amazon-bedrock traceability

Pinned source: `packages/ai/src/providers/amazon-bedrock.ts` at
`532f7d12cd19235081bd4d6e425bde185652d86f`.
Rust: `crates/pi-ai/src/providers/amazon_bedrock.rs`.
Tests: `crates/pi-ai/tests/provider_auth.rs`,
`crates/pi-ai/tests/differential_providers.rs` (`auth_resolution_matches_ts`,
`provider_login_flows_match_ts`).

| TypeScript | Rust | Proof |
|---|---|---|
| `bedrockAuth.name` | `BedrockAuth::name` | `provider_metadata_matches_the_ts_factories` |
| `login` select (`bearer-token`, `aws-profile`, `credential-chain`) | `login` with `AuthPromptKind::Select` | `bedrock_login_covers_bearer_profile_and_chain_flows`, differential login |
| bearer branch → secret prompt → `{type:"api_key", key}` | `ApiKeyCredential { key: Some(key), env: None }` | same |
| profile branch → `info` event with credential-chain link → text prompt → `env.AWS_PROFILE` | `AuthEvent::Info` + `ApiKeyCredential { env: AWS_PROFILE }` | same |
| chain branch → text prompt → `{type:"api_key"}` | `ApiKeyCredential { key: None, env: None }` | same |
| unknown method → `Unknown Amazon Bedrock auth method: <method>` | `AuthError::failed` same message | same, differential login |
| `resolve` order: stored key → `AWS_BEARER_TOKEN_BEDROCK` → stored/ambient `AWS_PROFILE` → access keys → ECS relative/full URI → web identity → `undefined` | `resolve` | `bedrock_resolve_detects_stored_and_ambient_credentials`, differential auth (13 env matrices) |
| `amazonBedrockProvider()` metadata/catalog/bedrock api | `amazon_bedrock_provider` | provider metadata test; dispatch parity is covered by `differential_bedrock_converse_stream` because the TS path uses the AWS SDK |

## Branch notes

- Env reads are abort-checked before and after each `ctx.env`.
- Truthiness: empty env values are treated as unset (`env_value` filters empty
  strings), matching JS.
- A stored `AWS_PROFILE` in `credential.env` short-circuits the ambient lookup
  (`??` semantics) and reports `source: "stored credential"`.

## Gaps

- Dispatch request parity is not part of `provider_dispatch_matches_ts_requests`
  (the AWS SDK bypasses `globalThis.fetch`); the bedrock adapter has its own
  differential suite.

## Discrepancies

- None.
