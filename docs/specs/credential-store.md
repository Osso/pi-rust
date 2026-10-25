# In-memory credential store

Port frozen `packages/ai/src/auth/credential-store.ts` at `4f9e6f8a4`. [Traceability](../porting/traceability/auth-credential-store.md) maps its operations.

## What it must do

- [ ] Serialize modify/delete operations per provider during normal awaiting, while reads and other providers remain independent.
- [ ] Invoke modifiers with the current value after earlier writes complete. Store a returned credential; undefined retains and returns the current credential, including an absent entry.
- [ ] Preserve stored values when a modifier rejects, propagate the rejection and let later queued operations complete. Delete removes the entry; later writes can recreate it.

## How it works

- [Module traceability](../porting/traceability/auth-credential-store.md).

## Implementation inventory

- `crates/pi-ai/src/auth/credential_store.rs` — in-memory values and provider-local serialization.

## Tests asserting this spec

- `crates/pi-ai/tests/credential_store.rs` — gated native queues compared with actual frozen queues.
- `crates/pi-ai/tests/auth_types_contract.rs` — original callback rejection preservation.

## Known gaps (current cycle)

Current queue/failure proof is pending.

## Out of scope

Cold/dropped Rust futures do not retain JavaScript Promise invocation/execution semantics; this is an approved language-boundary gap (reaffirmed2026-10-25). Values follow the approved owned-copy rule; persistent storage and its locking are application-owned.
