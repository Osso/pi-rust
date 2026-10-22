# providers/cloudflare-stream traceability

Pinned source: `packages/ai/src/providers/cloudflare-stream.ts` at
`532f7d12cd19235081bd4d6e425bde185652d86f`.
Rust: `crates/pi-ai/src/providers/cloudflare_stream.rs`.
Tests: `crates/pi-ai/tests/cloudflare_stream.rs`,
`crates/pi-ai/tests/differential_providers.rs` (`cloudflare_model_resolution_matches_ts`).

| TypeScript | Rust | Proof |
|---|---|---|
| `resolveCloudflareModel(model, env)` | `resolve_cloudflare_model` | `materializes_the_model_endpoint_before_dispatch`, `keeps_placeholders_when_the_provider_env_does_not_resolve_them`, `resolves_each_placeholder_independently`, differential |
| `cloudflareStreams(streams)` | `CloudflareStreams` + `cloudflare_streams` | same behavioral test through `ProviderStreams::stream`/`stream_simple` |

## Branch notes

- No env → the model is returned unchanged (Rust returns a clone; object identity
  is not observable).
- `replaceAll` semantics for `{CLOUDFLARE_ACCOUNT_ID}` and
  `{CLOUDFLARE_GATEWAY_ID}`; unresolved placeholders stay literal.
- The wrapper only defines `stream`/`streamSimple`, so deferred capabilities stay
  absent (Rust defaults to unsupported).

## Gaps

- None.

## Discrepancies

- None.

## Q18 provider request integration (2026-10-22)

Cloudflare account/gateway substitution uses native JsString replacement over the model endpoint; no scalar URL shadow is retained.

Compiler checkpoint `pi-ai-slice-final.log` at `0c71c960` reached seven errors, including two owned sites fixed by `7780fac1`; remaining auth-owner errors block library acceptance. `lossless_sdk_boundaries` exercises public-provider header rejection and URL conversion; authored, not yet executed at this checkpoint.
