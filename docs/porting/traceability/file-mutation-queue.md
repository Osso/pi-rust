# Agent-core traceability

## Agent-core `file_mutation_queue`

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

## Coding-agent `file_mutation_queue`

Pinned source: `packages/coding-agent/src/core/tools/file-mutation-queue.ts` at immutable `4f9e6f8a4`. Agent-core evidence above remains separately scoped.

Rust: `pi_coding_agent::core::tools::file_mutation_queue::with_file_mutation_queue`.

| TypeScript source | Observable behavior | Rust proof |
|---|---|---|
| `file-mutation-queue.ts:4-5` | Module-global queue map plus a registration queue | `QUEUES` map and `REGISTRATION` mutex, both process-global |
| `file-mutation-queue.ts:7-14` | Only `ENOENT`/`ENOTDIR` are missing-path errors | Native realpath error code classification; sixteen-case frozen write filesystem differential includes ENOTDIR fallback, NUL rejection and ELOOP propagation |
| `file-mutation-queue.ts:16-26` | Key = `resolve` then async `realpath`; missing entries use the resolved path | `mutation_queue_key` uses shared Node path resolve/realpath with original native error carrier; `node_realpath` checks both bindings against Node and `file_mutation_queue` proves symlink alias serialization |
| `file-mutation-queue.ts:32-61` | Registration includes async key resolution; await previous tail, run, release in finally, delete only current tail | `registration()` covers async key resolution and map insertion; release channel wakes the next waiter; `QueueGuard::drop` removes only its generation. Operation failure/panic drops the sender; native gated write proof and five queue tests exercise ordering/release |
| `file-mutation-queue.ts:30-56` | Same-file operations serialize; different files run in parallel | `tests/file_mutation_queue.rs`: same-file ordering with peak concurrency 1, parallel files reaching peak 2 through a two-party barrier, and eight waiters with peak 1 |

`tests/file_mutation_queue.rs` ports the three `withFileMutationQueue` cases of `test/file-mutation-queue.test.ts` (same file, different files, symlink alias) and adds failure-release and many-waiter cases.

## Differential status

Actual-frozen `differential_write_queue` compares deterministic gated interleavings and final file effects in nine scenarios (including invalid queue key winning over pre-abort); abort cannot release the queue while mkdir/write remains pending, operation failure precedes abort, and independent files progress. Native waiter proof includes a later independent registration, not merely a first Pending at async realpath. `differential_write_filesystem` compares sixteen local write scenarios; RED exposed generic std NUL/ELOOP errors at key resolution, GREEN uses the [shared Node realpath boundary](../../specs/node-realpath.md). `write-realpath-green-4.log`: ten focused tests passed across six targets. Earlier queue-only evidence: `write-queue-green.log`.

## Rust boundary

`with_file_mutation_queue` returns `io::Result<T>` where the error is only a path-resolution failure (the source throws from `getMutationQueueKey` before `fn` runs); the operation's own result `T` is returned unchanged, so failures and panics inside the operation behave like the source's `try`/`finally`.
