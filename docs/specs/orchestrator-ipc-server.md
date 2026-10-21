# Orchestrator ipc-server
Port of frozen `packages/orchestrator/src/ipc/server.ts`.

## What it must do
- [x] Exchange JSONL bidirectionally with frozen peers.
- [x] Pin frozen coalesced initial rpc_stream stall as an approved TS discrepancy.
- [ ] Approved post-pin fix: drain all complete buffered stream frames immediately, skip blank prefixes, retain incomplete tails.
- [ ] Prove remaining failure and callback branches.

## How it works
- [Traceability](../porting/traceability/orchestrator-ipc-server.md)

## Implementation inventory
- `crates/pi-orchestrator/src/ipc/server.rs`: Unix socket transport.

## Tests asserting this spec
- `crates/pi-orchestrator/tests/ipc.rs`

## Known gaps (current cycle)
- [ ] Complete exception/EOF/event branch fixtures.

## Out of scope
Live sockets and host state.
