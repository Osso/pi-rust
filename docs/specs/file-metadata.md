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
- [ ] Compare the public TypeScript type contract and binary64 values against the pinned source.

## How it works

- [Filesystem metadata traceability](../porting/traceability/file-metadata.md)

## Implementation inventory

- `crates/pi-agent-core/src/harness/types.rs` — `FileKind` and `FileInfo` definitions.
- `crates/pi-agent-core/src/harness/mod.rs` — public harness re-exports.
- `crates/pi-agent-core/tests/file_info.rs` — focused value and bit-preservation proof.
- `run-tests.sh` — focused test-runner registration.

## Tests asserting this spec

- `crates/pi-agent-core/tests/file_info.rs`

## Known gaps (current cycle)

- [ ] Add strict pinned TypeScript type and binary64 differential proof.

## Out of scope

- Node filesystem adapters, `Stats` classification, unsupported-type errors, lstat, path normalization, and filesystem I/O.
- Directory listing, symlink resolution, cancellation, async traits, session integration, and tool behavior.
- Constructors, validation, parsing, serde, fallback APIs, and additional dependencies.
