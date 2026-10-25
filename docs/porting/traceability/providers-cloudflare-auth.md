# providers/cloudflare-auth traceability

Source: packages/ai/src/providers/cloudflare-auth.ts at [common pin](../source-lock.json).
Native: crates/pi-ai/src/providers/cloudflare_auth.rs.

| Frozen behavior | Native | Proof |
|---|---|---|
| :10–19 stored credential owns key/env, no ambient merge | resolve_value | partial stored credential + configured ambient fixture, provider_auth regression |
| :22–30 global placeholder replacement, missing workers gateway becomes empty | materialize_base_url | differential_providers repeated-placeholder workers fixture |
| :32–53 reject missing/empty required values; return env/baseUrl/source | resolve_cloudflare_env | complete ten-case actual-source auth matrix; a12f44de RED before port |
| :55 workers name/login and apiKey/baseUrl | CloudflareWorkersAiAuth | full auth + login differential |
| :75 gateway name/login, null default-auth headers, scoped bearer/baseUrl | CloudflareAiGatewayAuth | full auth + login differential |

Legacy no-model status wrappers are consumers outside this module; their separate [status](../../specs/auth-resolution.md#approved-staging-gap-2026-10-22) is not proof or a blocker for these two model-aware auth constructors.

The absent [old stream wrapper](providers-cloudflare-stream.md) is removed from both factories; actual API dispatch is direct. Historical integration is recorded in the [Q15 proof](../q15-sdk-provider-proof.md). Current whole-module proof at ac6fc9c47: actual-source auth/login differential1 (ten resolution cases plus login), native auth4 and lossless login1 GREEN; `/tmp/ai-auth-closure-cloudflare-*.log`. Required two-package all-target check remains current at unchanged Rust revisiond6dd70086. No production change or new gap; status done.
