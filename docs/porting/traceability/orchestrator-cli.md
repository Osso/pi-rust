# Orchestrator cli

Source: `packages/orchestrator/src/cli.ts` at the [common pin](../source-lock.json). Rust: `crates/pi-orchestrator/src/cli.rs`.
Status: **partial**. [Q3 proof and native approvals](../orchestrator-proof.md).

| Export/branch | Behavioral proof |
| --- | --- |
| printHelp; package version; no args/help/version/unknown/usage | actual frozen subprocess stdout/stderr/exit codes compared byte-for-byte: cli |
| getFlagValue; list/spawn/status/stop/rpc; printResponse | first-flag semantics, default cwd and JSON request/pretty response bytes compared to frozen: cli |
| rpcStream | frozen/native complete JSONL stdin, blank lines, unterminated EOF tail, stdout/stderr and exit 0: cli |
| main serve and native propagated errors | approved library run entry returns code rather than exiting; complete native serve-CLI embedding matrix remains unproven |

## Explicit proof gaps
- The native CLI Serve branch is composed from tested runtime helpers, but lacks a direct embedding-process CLI serve/error exit-code differential.

`rpc_stream_only_forwards_complete_jsonl_stdin_lines` additionally compares BOM-padded commands and whitespace-only frames (ECMAScript non-ASCII whitespace set) against the frozen CLI. Replacing JS trim with Rust trim is killed. Negative CLI Unicode frames are not separately proved.

Native UTF-16 integration: [lossless orchestrator proof](lossless-orchestrator.md).

New native CLI embedding/signal proof: `cli_serve_process::cli_serve_signals_match_frozen` runs the actual frozen CLI and native run_cli in owned subprocesses. It compares startup log bytes and live list IPC, then verifies SIGINT/SIGTERM exit0 and socket removal. Process-global signal work has a standalone target; its only ignored function is the parent-invoked child helper. Execution pending at implementation commit.
