# RPC mode

Port the headless JSONL server in frozen `packages/coding-agent/src/modes/rpc/rpc-mode.ts`. [Traceability](../porting/traceability/rpc-mode.md) records exact branch coverage and source discrepancies.

## What it must do

### Commands and input
- [x] Emit command responses and original session events, retain raw UTF-16 values, and use the approved fixed native AssistantMessage order. Existing actual-source command-matrix, raw-id, continuation, compaction-failure and callable-status tests cover their named paths.
- [x] Execute RPC `bash` directly, without the old-only `user_bash` extension hook.
- [x] Echo an unknown string command and its error text without losing raw UTF-16 units.
- [x] Dispatch external JSON using literal frozen JsValue semantics; preserve raw ids/types and RPC-local strict equality, truthiness and ToString throws. Retain typed native client APIs.

### Lifecycle
- [x] Take over stdout before extension binding, using the shared output authority.
- [x] Remove subscriptions from their original owner when rebinding; preserve unrelated replacement-session observers.
- [x] Register signal listeners before startup can proceed; a first-turn SIGTERM/SIGHUP disposes and exits normally with143/129.
- [x] Match signal/default-handler exit behavior during pending disposal; keep the receiver responsive for repeated signals and skip duplicate child cleanup.

### Extension UI
- [x] Evaluate callable statuses immediately with no session override; emit an omitted statusText for undefined, and propagate the original throw. See [status contract](extension-status.md).
- [x] Preserve dialog response/abort/timeout notification order, cancellation precedence and empty-reply defaults; ignore unsupported factories and reject both theme-setter overloads consistently.
- [x] Emit exact fire-and-forget UI requests without retaining editor state; poll asynchronous shutdown requests after the next input, preserving the classified discrepancy.

## How it works
- [Source/branch traceability](../porting/traceability/rpc-mode.md)
- [JSONL framing](rpc-jsonl.md)

## Implementation inventory
- `crates/pi-coding-agent/src/modes/rpc/rpc_mode.rs`: RPC lifecycle, commands, extension UI and output.
- `crates/pi-coding-agent/src/modes/rpc/jsonl.rs`: input framing and serialization authority.
- `crates/pi-coding-agent/src/modes/rpc/rpc_mode/input.rs`: literal server ingress and approved per-field core boundary checks.
- `crates/pi-coding-agent-base/src/core/output_guard.rs`: shared takeover/output/backpressure authority.

## Tests asserting this spec
- `crates/pi-coding-agent/tests/differential_rpc_mode.rs`: actual frozen and native processes.
- `crates/pi-coding-agent/src/modes/rpc/rpc_mode.rs`: dialog behavior tests.
- `oracle/rpc-mode-fixture.mts`: actual-source extension fixture.

## Known gaps (current cycle)
- Approved deep-core/image/return-field representability gaps and runtime timer-warning gap are listed per field in [traceability](../porting/traceability/rpc-mode.md); no silent coercion or drop.
- Whole-module behavior proof: 29 actual-source/native process cases and 9 owned unit/surface/ordered-process cases. Checkpoint gates are recorded separately.

## Out of scope
- The 2026-10-24 RPC ingress ruling supersedes a blanket typed-command parsing exclusion: apply a simple first-core JS conversion where defined; otherwise reject deep wrong-typed core inputs explicitly and list every affected field as an approved gap in traceability. RPC-local operations remain exact for all JSON values.
- Client/type redesign, unrelated provider behavior and live external services are not part of this module slice.
