# Coding-agent file-mutation-queue traceability

Pinned source: `packages/coding-agent/src/core/tools/file-mutation-queue.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Rust: `pi_coding_agent::core::tools::file_mutation_queue::with_file_mutation_queue`.

| TypeScript source | Observable behavior | Rust proof |
|---|---|---|
| `file-mutation-queue.ts:4-5` | Module-global queue map plus a registration queue | `QUEUES` map and `REGISTRATION` mutex, both process-global |
| `file-mutation-queue.ts:7-14` | `ENOENT`/`ENOTDIR` are missing-path errors | `is_missing_path_error` matches `NotFound`/`NotADirectory` |
| `file-mutation-queue.ts:16-25` | Key = `resolve` then `realpath`, falling back to the resolved path for missing entries | `mutation_queue_key` (`std::path::absolute` + `canonicalize`, fallback on missing); focused symlink-alias test proves aliases share one queue |
| `file-mutation-queue.ts:30-56` | Registration is serialized; each call waits for the current tail, then runs; release happens in `finally`; the tail is deleted only when still current | `registration()` serializes the map read-modify-write; each call awaits the previous tail's release channel; `QueueGuard::drop` sends the release and removes the entry only when its generation is still current, so failures and panics release the queue |
| `file-mutation-queue.ts:30-56` | Same-file operations serialize; different files run in parallel | `tests/file_mutation_queue.rs`: same-file ordering with peak concurrency 1, parallel files reaching peak 2 through a two-party barrier, and eight waiters with peak 1 |

`tests/file_mutation_queue.rs` ports the three `withFileMutationQueue` cases of `test/file-mutation-queue.test.ts` (same file, different files, symlink alias) and adds failure-release and many-waiter cases.

## Differential status

No oracle: the contract is concurrency ordering, and a differential run cannot compare interleavings deterministically. The source's remaining tests in that file cover the edit/write tools, which are a different module.

## Rust boundary

`with_file_mutation_queue` returns `io::Result<T>` where the error is only a path-resolution failure (the source throws from `getMutationQueueKey` before `fn` runs); the operation's own result `T` is returned unchanged, so failures and panics inside the operation behave like the source's `try`/`finally`.
