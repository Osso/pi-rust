# Orchestrator serve

Source: all 77 lines of `packages/orchestrator/src/serve.ts` at the [common pin](../source-lock.json), `4f9e6f8a4`. Rust: `crates/pi-orchestrator/src/serve.rs`.
Status: **done**, whole-module audit completed 2026-10-25 under the lighter-proof rule. [Native Result/CLI boundary approval](../orchestrator-proof.md) and [lossless integration](lossless-orchestrator.md).

| Export/branch | Audit and behavioral proof |
| --- | --- |
| serve startup | mkdir, IPC server before recovery, sequential recovery and optional Radius registration, exact enabled/disabled/machine/listening logs audited; existing startup/fatal tests and native enabled-Radius full lifecycle |
| startup rejection | Close server, remove socket, rethrow; existing invalid-storage startup and native/frozen mkdir failure tests |
| shutdown | Single cached completion, close IPC before supervisor shutdown before Radius stop, socket cleanup, exit result; existing idempotence and fatal tests plus enabled-Radius real IPC/registration/disconnect order |
| signals/fatal errors | Native run_until_signal Result and CLI 0/1 mapping; existing owned subprocess SIGINT/SIGTERM and fatal-channel tests, frozen log/stderr/exit/socket comparisons |

## Successor proof

- `/tmp/runtime-successor-orchestrator.log`: 10 existing supervisor/serve tests and two native runtime-services tests PASS. The enabled-Radius test uses injected HTTP Fetch, a real fake RPC child, real Unix IPC, real persistence, exact startup logs and ordered Pi/machine disconnect. It calls shutdown twice and proves no duplicate HTTP disconnect or remaining socket/instance records.
- `/tmp/runtime-successor-orchestrator-signals.log`: native/frozen CLI SIGINT and SIGTERM lifecycle test and startup failure test PASS; the one ignored entry is the parent-invoked subprocess helper. Both signal kinds prove live list IPC before signal, exit 0, exact logs and removed socket. No new frozen differential/matrix.
- Existing reciprocal IPC and exact-UTF-16 wire proof also PASS in `orchestrator.log`; see [supervisor](orchestrator-supervisor.md).

The previous enabled-path and direct-normal-signal proof blockers are closed. No production serve discrepancy was found in this audit.

## Discrepancies / gaps

- DISCREPANCY (preserved frozen behavior): shutdown rejection stops later cleanup rather than retrying it; startup rejection closes IPC/removes its socket but does not run supervisor.shutdown or radius.stop. No new recovery path is added.
- GAP: malformed/adversarial records, numeric extremes and exceptional OS/runtime/scheduling fault shapes use the approved input/runtime gap; existing exact UTF-16 carriers remain.

Historical mkdir RED→GREEN: `bb6b06de0`; normal-signal CLI proof first passed at `85a190062`. Current successor proof above supersedes the old open-matrix notes; source inventory line counts are unchanged.
