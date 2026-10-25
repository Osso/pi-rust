# coding-agent child-process traceability

## Accepted runtime gap: child envp order

| Boundary | Evidence and disposition |
|---|---|
| Child envp order is sorted by std Command; frozen uses JS own-key order — accepted runtime gap, user decision 2026-10-24 (AGENTS.md; parent2d1d3eba) | Frozen `/proc/self/environ` yields `2,10,Z_FIRST,A_SECOND`; native and direct std Command (without a native input map) yield `10,2,A_SECOND,Z_FIRST`. Original behavioral RED remains `/tmp/pi-rust-repin-core-tools/bash-envp-order-red-2.log`. Active `differential_bash_environment::native_child_envp_is_sorted_and_frozen_own_key_order_is_preserved_as_evidence` asserts both actual orders, identical complete names/values, and the direct std control. No order-preserving backend is added. This applies to std-based child spawns, not just bash; pre-spawn validation still follows JS own-key order. |

Accepted-gap checkpoint `b448fe01`: `cargo test -j4 -p pi-coding-agent --test differential_bash_environment` passes 6/6 with no ignored tests (`envp-gap-runtime.log`). Target warning Clippy has 0 findings in the test; workspace all-target keep-going check has 0 errors/warnings; inventory remains PASS 686/186679, 454 done / 19 gap / 46 partial / 167 todo. Logs are under `/tmp/pi-rust-repin-core-tools/`. Production spawn and pre-spawn validation code are unchanged.

## Shared live-child SIGTERM

`send_child_sigterm` moves find's existing helper unchanged into this authority. It consults the live handle, never caches a PID across reaping, and treats an already-gone process as a completed signal attempt. Find/grep retain distinct abort/close policies. Grep4 frozen child histories and find3 regressions GREEN (`grep-child-abort-green.log`), including closed pipes and inherited stderr after parent exit.

## Shared per-data text reader

`read_utf8_chunks` moves the existing find stderr byte/read/repair loop unchanged into this authority for find/grep reuse. Callers retain their existing lifecycle/stop/error boundaries; no new capture loop or timer policy. Grep20 protocol +34 numeric +FD42 actual-process cases GREEN3/0 (`grep-protocol-green.log`), including malformed stderr bytes. No whole child-process source re-pin claim.

## Footer explicit capture signal prerequisite (2026-10-24)

The [capture contract](../../specs/child-process-capture.md) now takes an explicit kill signal and destroys/awaits readers before signaling/reaping on overflow. Existing wait/stream/RTK callers explicitly retain SIGKILL; footer execFile uses SIGTERM. Actual Node/Python trap RED→GREEN proves TERM and closed stdout (EPIPE), with a KILL regression and current10 child-process/3 RTK controls GREEN. See the [footer checkpoint proof](../footer-data-provider-proof.md) for revisions/artifacts. No timeout/escalation, duplicate reader, dependency or whole child-process re-pin claim is added.

## Current ls bounded-capture prerequisite

The existing wait/read authority now exposes [configured capture](../../specs/child-process-capture.md), with independent stdout/stderr byte caps, early termination/reaping and explicit read errors. `wait_for_child_process` delegates with no cap/signal; all six existing process-lifetime tests remain GREEN. Actual frozen ordinary RTK outputs and native UTF-8/termination safety are GREEN in `ls-rtk-io-green-2.log`. No duplicate reader loop or Node chunk-prefix oddity reproduction; whole child-process module re-pin remains separately scoped.

Pinned source: `packages/coding-agent/src/utils/child-process.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Differential test skipped: behavior depends on process lifetime, pipes, and timing; proven behaviorally in `crates/pi-coding-agent/tests/child_process.rs`.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `child-process.ts:6-13` | Spawn a process (cross-spawn on Windows) | `utils::child_process::spawn_process`; focused `captures_output_and_exit_code`; Windows cross-spawn is a documented gap |
| `child-process.ts:19-23` | Synchronous spawn returning status/stdout/stderr | Replaced by one `spawn_sync` authority; focused sync test preserves cwd/status/output; queue8 explicit limit/error proof below |
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

## Explicit sync limits (queue8,2026-10-24)

[Contract](../../specs/child-process-sync.md). Parent approved replacing old spawn_process_sync (deleted; no adapter) with one native spawn_sync authority. shell/project-resolver mechanical callers and existing cwd/CLI fixtures migrate. Clipboard actual-source RED `clipboard-module-red.log` proves ignored stderr overflow and missing SIGTERM. Direct13-case source/native proof covers explicit stdout/stderr/combined ENOBUFS, timeout SIGTERM/custom SIGKILL, closed pipes, nonzero/signal exits, missing spawn, invalid cwd/argv, maxBuffer0 and native error shape; `spawn-sync-result-red.log`/`-green.log`. Both production callers, existing cwd/status and CLI-overlay fixtures pass bounded regressions; final commands/revisions in `/tmp/pi-rust-repin-lossless-interactive/clipboard-module-proof-ledger.md`. Existing unbounded implicit-limit policy remains under the approved runtime-oddity exclusion; explicit clipboard limits do not. Async capture is unchanged.


## Queue10 shared stdio/input prerequisite (2026-10-24)

Parent grants explicit pipe/ignore/null stdio for the existing async/sync spawn authorities, plus sync input bytes; no second spawn path. Null means Node's pipe default; the existing consumers explicitly retain ignored stdin and piped outputs through CAPTURE defaults. Ignored result streams are null, never empty captured buffers. Sync input is written concurrently with output draining and exit/deadline polling; the same configured timeout includes blocked input delivery, and existing kill/error policies remain unchanged. [Contract](../../specs/child-process-sync.md).

Four new private-process behavioral REDs (`clipboard-text-shared-stdio-red.log`) prove missing piped input, incorrectly captured ignored output and whole-call timeout contract. GREEN at3ef587ef: async pipe/NUL input with ignored outputs; sync pipe input+null stdout/ignored stderr; both outputs ignored;4MiB blocked input with100ms SIGTERM/ETIMEDOUT. Actual Node spawnSync differential: new3 stdin/stdio/timeout groups plus prior13 limits/error groups GREEN2/2 (`clipboard-text-shared-stdio-node.log`). No source-fallback assertion changed.

Existing process target:9/10 GREEN initially, with post-exit descendant test observing only HEAD; stdin/stdout/stderr configuration is unchanged under CAPTURE. Added raw-stderr failure context without altering expected bytes; focused same descendant test then GREEN (`clipboard-text-shared-descendant-diagnostic.log`). This is a recorded timing-sensitive observation, not evidence of a diagnosed new cause or justification for changing the existing100ms grace policy. Composite10/10 covered; no broad rerun merely to erase the transient result.
