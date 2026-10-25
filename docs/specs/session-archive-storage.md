# Session archive storage

The six frozen `core/session-archive-storage.ts` exports archive and restore session files without changing transcript bytes. [Traceability](../porting/traceability/session-archive-storage.md).

## What it must do

- [x] Compress and restore bidirectionally with TypeScript while preserving decompressed bytes and file modes.
- [x] Roll back archive or restore publication when metadata relocation fails; preserve a newer surviving plain transcript.
- [x] Preserve original filesystem, decoder and metadata Error constructors, messages and properties.
- [x] Match missing-file, already-archived, plain-file and existing-target short circuits without replacing input files.

## How it works

- [Archive storage trace](../porting/traceability/session-archive-storage.md).
- [Control/store contract](control-store-repin.md).

## Implementation inventory

- `crates/pi-coding-agent-storage/src/core/session_archive_storage.rs` — file and metadata operations.
- `crates/pi-coding-agent-storage/src/core/session_manager.rs` — existing control error carrier adapter.

## Tests asserting this spec

- `crates/pi-coding-agent/tests/repin_storage_archive.rs` (module in `suite_session_1`).
- `crates/pi-coding-agent/tests/repin_session_storage.rs` — reciprocal bytes, modes, rollback and plain precedence.
- `oracle/storage-archive-oracle.mts` — frozen operational exports and exact error projection.

## Known gaps (current cycle)

Current focused matrix, reciprocal tests, protected fixture checks, scoped lint and storage check pass. Whole manager/control acceptance remains separate.

## Out of scope

Exceptional OS faults and runtime races follow repository parity exclusions. Whole manager/control acceptance is separate.
