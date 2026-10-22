# api/cloudflare traceability

Pinned source: `packages/ai/src/api/cloudflare.ts` at commit `532f7d12cd19235081bd4d6e425bde185652d86f`.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `CLOUDFLARE_WORKERS_AI_BASE_URL` | Workers AI direct endpoint template. | `api::cloudflare::CLOUDFLARE_WORKERS_AI_BASE_URL`; `exports_the_documented_cloudflare_endpoints`. |
| `CLOUDFLARE_AI_GATEWAY_COMPAT_BASE_URL` | Gateway Unified API template. | `CLOUDFLARE_AI_GATEWAY_COMPAT_BASE_URL`; same test. |
| `CLOUDFLARE_AI_GATEWAY_OPENAI_BASE_URL` | Gateway OpenAI passthrough template. | `CLOUDFLARE_AI_GATEWAY_OPENAI_BASE_URL`; same test. |
| `CLOUDFLARE_AI_GATEWAY_ANTHROPIC_BASE_URL` | Gateway Anthropic passthrough template. | `CLOUDFLARE_AI_GATEWAY_ANTHROPIC_BASE_URL`; same test. |
| Differential oracle | Exact exported strings. | `tests/differential_api_leaf.rs` compares all four values. |

## Exclusions

- None. Value-only module with byte-exact differential proof.

Q18 native auth integration and bounded proof status: [lossless auth](lossless-auth.md).
