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
