# runtime credentials traceability

Pinned source: `packages/coding-agent/src/core/runtime-credentials.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.
Rust: `crates/pi-coding-agent/src/core/runtime_credentials.rs`.
Focused tests: `tests/runtime_credentials.rs` (5 cases, ports of `test/runtime-credentials.test.ts`).
Differential test skipped: the overlay is pure in-memory delegation; every observable effect is
covered by the focused tests against the real `AuthStorage`.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `runtime-credentials.ts:5-18` | Overrides keyed by provider; set/remove/has | `set_runtime_api_key`, `remove_runtime_api_key`, `has_runtime_api_key`; masking test |
| `runtime-credentials.ts:20-31` | `read` returns the override as an api-key credential, otherwise delegates | `read`; masking test |
| `runtime-credentials.ts:33-43` | `list` merges overrides without keys, preserving store order | `list`; enumeration test |
| `runtime-credentials.ts:45-51` | `modify` forwards to the store | `modify`; signal-forwarding test |
| `runtime-credentials.ts:53-58` | `delete` clears the stored credential and only then the override | `delete`; cancellation test, delete test |
| `runtime-credentials.ts:59-63` | `reload` forwards when the store supports it | not ported: `CredentialStore` has no `reload`; `AuthStorage::reload` is called directly by owners |

## Gaps

- `reload` forwarding is a TypeScript duck-typing convenience; the Rust `CredentialStore` trait has no
  such method, so callers hold `AuthStorage` and call `reload()` on it.

## Discrepancies

- None observed.

## Proof inventory

- Focused cases: 5.
