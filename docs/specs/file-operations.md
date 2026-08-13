# Agent-core file-operation formatting

This bounded slice ports the pure file-summary formatter from `packages/agent/src/harness/compaction/utils.ts:61-73`. File-operation accumulation, extraction, serialization, and compaction orchestration remain deferred. See the [file-operation traceability](../porting/traceability/file-operations.md).

## What it must do

- [x] Expose `format_file_operations` only through `pi_agent_core::harness::compaction::utils` with the source-shaped signature `(&[String], &[String]) -> String`.
- [x] Return `""` when both lists are empty.
- [x] Emit `<read-files>` only when the read list is non-empty.
- [x] Emit `<modified-files>` only when the modified list is non-empty.
- [x] Join each list's entries with LF and preserve order, duplicates, empty entries, Unicode, NUL, punctuation, XML-sensitive text, and embedded or trailing newlines.
- [x] Separate emitted sections with exactly two LF characters and prefix every non-empty result with exactly two LF characters.
- [x] Preserve raw entries without escaping, sorting, deduplication, path normalization, filesystem access, or other I/O.

## Boundary and proof

- [x] Keep the helper pure, standard-library-only, and module-only. Do not add a harness-root re-export, generic parameter forms, or adjacent compaction utilities.
- [x] Cover the focused contract in `crates/pi-agent-core/tests/file_operations.rs` and register it in `run-tests.sh`.
- [x] Compare direct differential request groups `13089..=13095` against the pinned TypeScript source.

## Proof status

- Production implementation: commit `54c5010`; focused proof has 7 passed tests.
- Differential proof: commit `84603a4`; requests `13089..=13095` pass against the pinned TypeScript source.
- Oracle typecheck: `/tmp/pi-rust-file-operations-oracle.out` records `./run-tests.sh oracle` exit 0.

## How it works

- [File-operation traceability](../porting/traceability/file-operations.md)

## Implementation inventory

- `crates/pi-agent-core/src/harness/compaction/utils.rs` — pure formatter.
- `crates/pi-agent-core/src/harness/compaction/mod.rs` — module-only boundary.
- `crates/pi-agent-core/tests/file_operations.rs` — focused behavioral/API proof.
- `crates/pi-agent-core/tests/differential_truncate/file_operations.rs` — seven pinned differential fixture groups (`13089..=13095`).
- `oracle/truncate-oracle.ts` — persistent direct `formatFileOperations` oracle operation.
- `run-tests.sh` — focused test registration.

## Tests asserting this spec

- `crates/pi-agent-core/tests/file_operations.rs`
- `crates/pi-agent-core/tests/differential_truncate.rs`
- `crates/pi-agent-core/tests/differential_truncate/file_operations.rs`

## Known gaps (current cycle)

None for this bounded formatter slice.

## Out of scope

File-operation accumulation or computation, extraction, serialization, message types, compaction algorithms, session mutation, filesystem access, path APIs, normalization, escaping, sorting, deduplication, fallbacks, and new dependencies.
