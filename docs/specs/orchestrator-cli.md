# Orchestrator cli

Port of frozen `packages/orchestrator/src/cli.ts`. Native boundary decisions and proof scope: [Q3 proof](../porting/orchestrator-proof.md).

## What it must do
- [x] rpc-stream stdin uses ECMAScript String.trim before parsing complete lines.
- [x] printHelp; package version; no args/help/version/unknown/usage: actual frozen subprocess stdout/stderr/exit codes compared byte-for-byte: cli.
- [x] getFlagValue; list/spawn/status/stop/rpc; printResponse: first-flag semantics, default cwd and JSON request/pretty response bytes compared to frozen: cli.
- [x] rpcStream: frozen/native complete JSONL stdin, blank lines, unterminated EOF tail, stdout/stderr and exit 0: cli.
- [ ] The native CLI Serve branch is composed from tested runtime helpers, but lacks a direct embedding-process CLI serve/error exit-code differential.

## How it works
- [Module traceability](../porting/traceability/orchestrator-cli.md)

## Implementation inventory
- `crates/pi-orchestrator/src/cli.rs`: native module.

## Tests asserting this spec
- `crates/pi-orchestrator/tests/cli.rs`
- `crates/pi-orchestrator/tests/supervisor.rs`

## Known gaps (current cycle)
- [ ] The native CLI Serve branch is composed from tested runtime helpers, but lacks a direct embedding-process CLI serve/error exit-code differential.

## Out of scope
Live services, user state and real Radius network calls; native packaging follows the approved library boundary.
