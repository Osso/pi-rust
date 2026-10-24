# providers/cloudflare-auth traceability

Source: packages/ai/src/providers/cloudflare-auth.ts at [common pin](../source-lock.json).
Native: crates/pi-ai/src/providers/cloudflare_auth.rs.

| Frozen behavior | Native | Proof |
|---|---|---|
| :10–19 stored credential owns key/env, no ambient merge | resolve_value | partial stored credential + configured ambient fixture, provider_auth regression |
| :22–30 global placeholder replacement, missing workers gateway becomes empty | materialize_base_url | differential_providers repeated-placeholder workers fixture |
| :32–62 reject missing/empty required values; return env/baseUrl/source | resolve_cloudflare_env | complete ten-case actual-source auth matrix; a12f44de RED before port |
| :65–89 workers name/login and apiKey/baseUrl | CloudflareWorkersAiAuth | full auth + login differential |
| :92–119 gateway name/login, null default-auth headers, scoped bearer/baseUrl | CloudflareAiGatewayAuth | full auth + login differential |

No-model legacy status wrappers retain the separately approved [staging gap](../../specs/auth-resolution.md#approved-staging-gap-2026-10-22); actual request models never use a substitute.

The absent [old stream wrapper](providers-cloudflare-stream.md) is removed from both factories; actual API dispatch is direct. Focused GREEN and shared workspace check remain to record.
