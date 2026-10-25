# Session history traversal

Search-only history follows frozen `core/session-history.ts` at the [common pin](../porting/source-lock.json). It must not install historical entries in model context. [Proof and branch map](../porting/traceability/session-history.md).

## What it must do

- [x] Yield the retained branch newest first, then follow exact parent IDs through the physical transcript, skipping inactive records.
- [x] Stop without opening the transcript when the first retained parent is absent or empty.
- [x] Read archived history without restoring or modifying the archive; bound memory by the read chunk and largest record; clean temporary plaintext after exhaustion or early drop.
- [x] Preserve original Node open errors when the transcript disappears after manager creation.
- [ ] Preserve original decoder Error, code and errno when an archive becomes corrupt after manager creation.

## How it works

- [History traceability](../porting/traceability/session-history.md).
- [Storage integration evidence](../porting/traceability/session-storage-integration.md).

## Implementation inventory

- `crates/pi-coding-agent-storage/src/core/session_history.rs` — lazy active-ancestry traversal.
- `crates/pi-coding-agent-storage/src/core/session_transcript_spool.rs` — shared error-preserving archive byte copy.

## Tests asserting this spec

- `crates/pi-coding-agent/tests/repin_storage_history.rs` (module in `suite_session_1`).
- `crates/pi-coding-agent/tests/repin_session_storage.rs` — compacted, large Unicode, memory and early-drop cases.
- `oracle/session-storage-oracle.ts` — actual frozen source producer and history reader.

## Known gaps (current cycle)

- [ ] Current decoder change needs focused GREEN and scoped lint proof.

## Out of scope

Adversarial persisted shapes and OS/runtime faults follow the repository parity exclusions. Whole manager/control-store acceptance is separate.
