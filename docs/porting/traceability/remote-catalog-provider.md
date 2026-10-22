# remote catalog provider traceability

Pinned source: `packages/coding-agent/src/core/remote-catalog-provider.ts` at
`532f7d12cd19235081bd4d6e425bde185652d86f`.
Rust: `crates/pi-coding-agent/src/core/remote_catalog_provider.rs`.

Differential test skipped: the module is asynchronous, drives a provider wrapper, and needs HTTP
responses; it is proven behaviorally with an injected `Fetch` stub
(`crates/pi-coding-agent/tests/remote_catalog_provider.rs`, 9 cases) so no real service is contacted.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `remote-catalog-provider.ts:8-18` | Merge dynamic models by id, replacing existing entries | `merge_models`; restore/merge test |
| `remote-catalog-provider.ts:20-37` | `parseCatalog`: array, `{models: []}`, or object values; entries must be objects with `id`; `provider` is overridden | `parse_catalog`; valid-catalog and invalid-shape tests |
| `remote-catalog-provider.ts:39-48` | Overlay suppressed when `lastModified <= localGeneratedAt` | `remote_models`; newer-generation test |
| `remote-catalog-provider.ts:50-132` | Wrapper provider: restored overlay publish, offline/abort exit, four-hour freshness window, conditional GET, 304/404/501/transient handling, persist + update | `RemoteCatalogProvider`; 9 behavioral cases |
| `remote-catalog-provider.ts:77-82` | Validator only when a cached body exists | `validator` filter; forced-refresh test asserts `if-none-match` |
| `remote-catalog-provider.ts:86-91` | URL uses `encodeURIComponent(providerId)` | `URI_COMPONENT` set; URL assertion in the forced-refresh test |
| `remote-catalog-provider.ts:104-110` | Transient failures keep the cached body and etag, then throw `Model catalog request failed for <id>: <status>` | same; transient test |
| `remote-catalog-provider.ts:112-131` | `Last-Modified` parsed to milliseconds (NaN → 0), etag stored verbatim, provider override | valid-catalog test (epoch 1_445_412_480_000) |

## Gaps

- Catalog entries that do not deserialize into `pi_ai::types::Model` are skipped; TypeScript spreads
  whatever object it received. This is the typed boundary for catalog data.
- `Date.parse` accepts several date formats; Rust uses `httpdate` (RFC 7231 dates), which covers the
  server's `Last-Modified` header.

## Discrepancies

- None observed for the ported flows.

## Proof inventory

- Behavioral cases: 9.

Q18 auth consumer propagation retains native key/base URL/env/header values; native errors avoid Display except explicit formatting. Tests/proof status: [lossless auth](lossless-auth.md).
