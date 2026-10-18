# coding-agent fs-watch traceability

Pinned source: `packages/coding-agent/src/utils/fs-watch.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Differential test skipped: filesystem notifications are OS- and timing-dependent; proven behaviorally in `crates/pi-coding-agent/tests/fs_watch.rs`.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `fs-watch.ts:3` | Retry delay is 5000 ms | `FS_WATCH_RETRY_DELAY_MS`; focused constant test |
| `fs-watch.ts:5-13` | Close a watcher, ignoring errors and null | `close_watcher` drops the watcher; focused no-op test |
| `fs-watch.ts:15-25` | Create a watcher, attach the error handler, report setup failure and return null | `watch_with_error_handler`; focused creation/modification and setup-failure tests |
| `fs-watch.ts:18` | Listener receives `rename`/`change` plus a filename | `WatchEventKind` mapping from notify event kinds; focused event test |

## Discrepancies

- The source uses `node:fs.watch`. The Rust port uses `notify`'s recommended backend with `RecursiveMode::NonRecursive`. Event-kind mapping is explicit: create/remove/rename → `Rename`, other modifications → `Change`.
- notify has no `close()`; dropping the watcher stops it, which is infallible.

## Proof inventory

- Focused cases: 4.
