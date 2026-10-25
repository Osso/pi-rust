# Orchestrator cli

Source: `packages/orchestrator/src/cli.ts` at the [common pin](../source-lock.json). Rust: `crates/pi-orchestrator/src/cli.rs`.
Status: **done** (realistic-input user-observable parity). [Q3 proof and native approvals](../orchestrator-proof.md).

| Export/branch | Behavioral proof |
| --- | --- |
| printHelp; package version; no args/help/version/unknown/usage | actual frozen subprocess stdout/stderr/exit codes compared byte-for-byte: cli |
| getFlagValue; list/spawn/status/stop/rpc; printResponse | first-flag semantics, default cwd and JSON request/pretty response bytes compared to frozen: cli |
| rpcStream | frozen/native complete JSONL stdin, blank lines, unterminated EOF tail, stdout/stderr and exit 0: cli |
| main serve and native propagated errors | Approved library run entry returns code rather than exiting; owned native/frozen subprocesses prove normal SIGINT/SIGTERM, live IPC, socket removal, and exact startup-error message/exit1. |

## Explicit proof gaps
- Process-global CLI serve and error behavior is now proved in cli_serve_process.rs; the only ignored function is its parent-invoked child helper.

`rpc_stream_only_forwards_complete_jsonl_stdin_lines` additionally compares BOM-padded commands and whitespace-only frames (ECMAScript non-ASCII whitespace set) against the frozen CLI. Replacing JS trim with Rust trim is killed. Negative CLI Unicode frames are not separately proved.

Native UTF-16 integration: [lossless orchestrator proof](lossless-orchestrator.md).

New native CLI embedding/signal proof: `cli_serve_process::cli_serve_signals_match_frozen` runs the actual frozen CLI and native run_cli in owned subprocesses. It compares startup log bytes and live list IPC, then verifies SIGINT/SIGTERM exit0 and socket removal. Process-global signal work has a standalone target; its only ignored function is the parent-invoked child helper. GREEN at bb6b06de0 (`/tmp/runtime-services-green-cli-serve.log`).

Whole frozen module re-read in the runtime-services lane (2026-10-25). No platform-fault or event-scheduling equivalence is claimed.
