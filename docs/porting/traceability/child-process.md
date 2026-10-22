# coding-agent child-process traceability

Pinned source: `packages/coding-agent/src/utils/child-process.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Differential test skipped: behavior depends on process lifetime, pipes, and timing; proven behaviorally in `crates/pi-coding-agent/tests/child_process.rs`.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `child-process.ts:6-13` | Spawn a process (cross-spawn on Windows) | `utils::child_process::spawn_process`; focused `captures_output_and_exit_code`; Windows cross-spawn is a documented gap |
| `child-process.ts:19-23` | Synchronous spawn returning status/stdout/stderr | `spawn_process_sync`; focused sync test with cwd |
| `child-process.ts:29-100` | After exit, keep reading until both pipes end or the idle grace expires; re-arm on every chunk; report exit code; destroy pipes | `wait_for_child_process` with a 100 ms re-armed deadline; focused post-exit descendant and quiet-descendant tests |
| `child-process.ts:222-227` | Process errors reject | `spawn_process` returns `io::Error`; focused `spawn_failures_are_reported` |
| `child-process.ts:229-241` | Exit and close report the exit code; signal termination reports null | Focused signal test (`status == None`) |
| `child-process.ts:222-227` | Spawn errors carry Node's `spawn <command> <ERRNO>` text | `node_spawn_error_message` maps `io::Error` errno codes through the Linux errno-name table; `package_manager` tests assert `spawn <path> ENOENT` and `spawn <path> EACCES` |

## Shared Node errno mapping (ca-errno)

Node/libuv reports `uv_err_name` (`ETXTBSY`, `ENOTDIR`, ...), not the OS description. One Rust
mapping serves every spawn and filesystem caller. Implementation now lives in `pi-js-compat::node_fs`; existing public paths re-export unchanged signatures. [Shared filesystem proof and gaps](node-fs-errors.md).

Existing public contracts:

| Item | Contract | Proof |
|---|---|---|
| `utils::child_process::node_errno_name(i32)` | Raw errno number to Node code, full Linux table, `None` outside `0..=4095` | `tests/errno_codes.rs` (ETXTBSY 26, ENOENT 2, EACCES 13, ENOTDIR 20) |
| `utils::child_process::node_error_code(&io::Error)` | Raw errno first; `ErrorKind` mapping only for errors std constructs without an errno | `tests/errno_codes.rs` |
| `node_spawn_error_message(prefix, command, error)` | `<prefix> <command> <ERRNO>`, falling back to the error display text only when there is no errno | `tests/errno_codes.rs`; package-manager spawn tests |
| Callers: pyrun runner, bwrap availability check, browser-cli, ask-secret, claude-memory enrich/session-end, rpc client, config spawnSync, session-selector trash, ls rtk, bwrap fs worker | All use the shared mapping instead of hand-rolled 2-3 entry matches that collapsed unknown codes to `EIO` | `tests/errno_codes.rs` (pyrun + bwrap at the real exec boundary); focused caller suites |

Discrepancy: `core::tools::node_fs::node_fs_error` uses Node's lowercase libuv descriptions only for
`ENOENT`/`EACCES`/`EISDIR`/`ENOTDIR`/`EEXIST`/`EINVAL`/`ENOTEMPTY`. The separately proven ELOOP description now also matches Node. Other codes report the
correct Node code but keep std's description (for example `ETXTBSY: Text file busy (os error 26), open 'x'`
instead of Node's `ETXTBSY: text file busy, open 'x'`). The code itself matches; a full `uv_strerror`
table is out of this slice's scope.

### Root cause

Three production callers (`pyrun/runner.rs::spawn_error`, `bwrap/backend.rs::spawn_sync_error`,
`core/tools/node_fs.rs::node_error_code`) and six further call sites duplicated a 2-3 entry
`ErrorKind` match ending in `_ => "EIO"`. Any real errno outside those entries (notably `ETXTBSY`
from the test-script spawn race) was reported as `EIO`, hiding the cause. The full table already
existed in `child_process.rs`; the fix exposes it and deletes the ad-hoc maps.

## Discrepancies

- The source leaves stdout/stderr consumption to callers listening on the streams. Rust ownership prevents shared pipe listeners, so `wait_for_child_process` drains and returns stdout/stderr together with the exit status.
- `cross-spawn` Windows semantics (PATHEXT resolution and shebang handling) are not ported; `std::process::Command` is used on every platform.
- The grace timer is behaviorally proven with real subprocesses rather than virtual timers.

## Proof inventory

- Focused cases: 6 (output/status, post-exit writes, quiet descendant, signal, spawn failure, sync spawn).
- Errno mapping: 6 behavioral cases in `tests/errno_codes.rs` (ENOENT, EACCES, ETXTBSY, ENOTDIR; pyrun and bwrap spawn paths).

## Q18 native spawn error contract (2026-10-22)

`node_spawn_error_message(prefix: &str, command: &JsString, error: &io::Error)`
returns `JsString`. The command is inserted natively into errno text; process
argv encoding is not reused for diagnostics. `spawn_error_message_preserves_unpaired_command_units`
uses a real failed spawn at the encoded filesystem path then checks original
command units in the native error. Written but unexecuted: coding compilation
blocks errno_codes. Literal command/direct wrapper call sites construct native
strings; wrappers and ConfigError message contracts widen, never encode at an
intermediate error adapter. Main/204/209 own downstream scalar consumer fallout.
