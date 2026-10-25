# Credential store — current interface migration

Source: `packages/ai/src/auth/credential-store.ts` at4f9e6f8a4; Rust `crates/pi-ai/src/auth/credential_store.rs`. Whole implementation revalidation remains separate from [auth/types closure](auth-types.md).

| Surface | Current native behavior / proof |
|---|---|
| read | Owned credential copy; auth/model request and expiry tests. Missing entry returns None. |
| modify | Reads current value after acquiring the per-provider mutex, invokes modifier once, stores only Some(next), otherwise retains current. Models concurrent-refresh and authoritative-lock-recheck tests exercise this boundary. |
| delete | Same per-provider serialization; actual logout/type-change lock-recheck scenarios exercise deletion. |
| failure | Callback Result propagates unchanged; app-adapter six-reason comparison uses the real frozen InMemoryCredentialStore. No operation-signal/list API remains. |

Removed old-only list/operation cancellation methods and their tests are listed in [auth/types](auth-types.md#explicitly-retired-old-only-testsassertions). Those historical cancellation/metadata tests are retired evidence, not new-pin proof.

Native values follow the approved owned-copy rule; no frozen built-in consumer relies on mutation through a returned reference. The in-memory implementation serializes within one process; persistent-store locking is backend-owned. [Queue contract](../../specs/credential-store.md): dedicated gated fixtures now compare actual native and frozen same-provider modify/delete order, read-through visibility, independent providers, queued failure recovery, undefined retention, delete and recreation. Original rejection-value proof remains the six-reason app-adapter comparison above. Current focused queue/failure proof is pending.

Native async lifecycle gap (parent reaffirmed2026-10-25): cold/dropped Rust futures do not preserve Promise invocation-time snapshots, call-time queue reservation or retained execution. Normal awaiting, provider-local serialization and failure recovery remain required.
