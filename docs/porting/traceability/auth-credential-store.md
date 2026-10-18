# credential store traceability

Source: `packages/ai/src/auth/credential-store.ts` → `crates/pi-ai/src/auth/credential_store.rs`.
Tests: `crates/pi-ai/tests/auth.rs`.

## Symbols → proof
| TS | Rust | Test |
|---|---|---|
| `InMemoryCredentialStore.read` | `CredentialStore::read` | `in_memory_store_lists_metadata_and_serializes_modifications`, `aborted_queued_mutation_never_runs` |
| `list` | `list` | `in_memory_store_lists_metadata_and_serializes_modifications` (metadata only, insertion order) |
| `modify` | `modify` | `aborted_queued_mutation_never_runs`, OAuth refresh tests |
| `delete` | `delete` | `models::runs_provider_login_and_logout_through_the_credential_store` |
| per-provider promise chain | per-provider fair `tokio::sync::Mutex` | `aborted_queued_mutation_never_runs` (aborted waiter never executes its modifier) |

## Semantics
- `modify` sees the current credential under the per-provider lock, checks the operation signal before and after the modifier, and stores only `Some(next)`.
- An operation whose signal aborts while queued is dropped before the modifier runs; this matches TS `enqueue`, where the queued task re-checks the signal after waiting.
- Entries live in a `Vec` so `list` preserves insertion order like the TS `Map`.
- Read/list return clones; callers cannot mutate stored credentials through them.

## Gaps
- Cross-process locking is store-implementation-owned in TS; the in-memory store only serializes within the process, as in TS.
