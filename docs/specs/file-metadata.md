# Agent-core filesystem metadata values

The agent-core port exposes the dependency-free `FileKind` and `FileInfo` value contracts from `packages/agent/src/harness/types.ts`; adapter behavior remains deferred. See the [filesystem metadata traceability](../porting/traceability/file-metadata.md) for source mappings.

## What it must do

### File kind

- [x] Preserve the exact `file`, `directory`, and `symlink` literals through `FileKind::as_str`.
- [x] Expose `FileKind` from `pi_agent_core::harness` with the selected closed-enum traits.

### File metadata

- [x] Expose mutable public `name`, `path`, `kind`, `size`, and `mtime_ms` fields in source order.
- [x] Preserve empty, Unicode, supplementary, embedded-NUL, and punctuation strings.
- [x] Preserve all selected JavaScript `number` bit patterns in `f64` `size` and `mtime_ms` fields.
- [x] Expose `FileInfo` from `pi_agent_core::harness` with `Debug` support.
- [x] Compare the public TypeScript type contract and binary64 values against the pinned source: strict generated type proof plus 10 runtime record fixtures (`13014..=13023`).

## How it works

- [Filesystem metadata traceability](../porting/traceability/file-metadata.md)

## Implementation inventory

- `crates/pi-agent-core/src/harness/types.rs` — `FileKind` and `FileInfo` definitions.
- `crates/pi-agent-core/src/harness/mod.rs` — public harness re-exports.
- `crates/pi-agent-core/tests/file_info.rs` — focused value and bit-preservation proof.
- `crates/pi-agent-core/tests/differential_truncate/file_info.rs` — 10 runtime record/binary64 differential fixtures (`13014..=13023`).
- `oracle/truncate-oracle.ts` — plain-record binary64 oracle operation; no `FileInfo` runtime constructor exists.
- `oracle/public-index-typecheck.mjs` — strict public-index type-only proof for metadata/resources and pinned HEAD check.
- `run-tests.sh` — focused and oracle test-runner registration.

## Tests asserting this spec

- `crates/pi-agent-core/tests/file_info.rs`
- `crates/pi-agent-core/tests/differential_truncate.rs`
- `crates/pi-agent-core/tests/differential_truncate/file_info.rs`
- `oracle/public-index-typecheck.mjs`

## Known gaps (current cycle)

None for this bounded value-contract slice.

## Out of scope

- Node filesystem adapters, `Stats` classification, unsupported-type errors, lstat, path normalization, and filesystem I/O.
- Directory listing, symlink resolution, cancellation, async traits, session integration, and tool behavior.
- Constructors, validation, parsing, serde, fallback APIs, and additional dependencies.
