# File mutation queue traceability

Pinned source: `packages/agent/src/harness/tools/file-mutation-queue.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Rust: `crates/pi-agent-core/src/harness/tools/file_mutation_queue.rs`.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `file-mutation-queue.ts:5-8` | Per-environment queue map plus serialized registration chain | `FileMutationQueue` owns a `tokio::sync::Mutex<QueueState>`; per-key `Arc<Mutex<()>>` plus a user count |
| `file-mutation-queue.ts:10-18` | State is created lazily per environment | Rust queue is owned by `ExecutionToolContext`, so one context shares one queue across its tools |
| `file-mutation-queue.ts:20-26` | Key = canonical path; `not_found`/`not_supported` fall back to the absolute path; other errors propagate | `mutation_queue_key`; exercised by write/edit tests and the aborted-write/edit paths |
| `file-mutation-queue.ts:29-55` | Mutations for the same key run one at a time; release happens in `finally`; empty entries are removed | `run`/`register`/`release`; `edit_serializes_through_canonical_and_symlink_paths` proves canonical aliasing |

## Rust boundary

- The source uses a `WeakMap<ExecutionEnv, ...>`; Rust makes the queue explicit in `ExecutionToolContext` because tools are constructed without a harness-provided per-call context.
- Entries track active users so a waiter keeps the same lock; the entry is removed when the last user releases it.
