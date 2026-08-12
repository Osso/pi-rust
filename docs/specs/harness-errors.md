# Agent-core harness errors

Agent-core exposes the typed filesystem, process-execution, compaction, and branch-summary errors required by harness capability contracts. The pinned source definitions are `packages/agent/src/harness/types.ts:137-211`; effectful filesystem, process, compaction, and branch-summary implementations remain later slices.

## What it must do

### Error codes and exports

- [x] Expose every `FileErrorCode` literal through a closed Rust enum and exact `as_str()` mapping.
- [x] Expose every `ExecutionErrorCode` literal through a closed Rust enum and exact `as_str()` mapping.
- [x] Export the filesystem and process-execution Rust types through `pi_agent_core::harness` and invoke their TypeScript classes through the public agent package index.
- [x] Export the compaction and branch-summary Rust types through `pi_agent_core::harness`; direct public-index differential proof remains pending.

### Error values

- [x] Preserve `FileError` public `name`, `message`, `code`, optional `path`, and optional typed `cause` fields.
- [x] Preserve `ExecutionError` public `name`, `message`, `code`, and optional typed `cause` fields.
- [x] Preserve source constructor ordering and exact initialized names.
- [x] Reflect current public message and cause fields through `Display` and `Error::source()`.
- [x] Preserve empty, Unicode, supplementary, and NUL-containing Rust strings.
- [x] Compare 14 normalized constructor values directly with the pinned TypeScript classes through `harness_error` requests `13000..=13013` in `oracle/truncate-oracle.ts`.

### Compaction and branch-summary errors

- [x] Expose distinct `CompactionErrorCode` and `BranchSummaryErrorCode` enums with exact `aborted` and `summarization_failed` literals.
- [x] Preserve `CompactionError` and `BranchSummaryError` constructor order, exact names, public message/code fields, and optional typed causes from `types.ts:183-211`.
- [x] Preserve `Display` and `Error::source()` behavior after public field mutation.
- [x] Prove both ordinary and cross-class nested cause chains with focused tests.
- [ ] Compare normalized constructor values directly with the pinned TypeScript classes in a later differential slice.

## How it works

- [Harness-error traceability](../porting/traceability/harness-errors.md)

## Implementation inventory

- `crates/pi-agent-core/src/harness/types.rs` — typed error codes and error values.
- `crates/pi-agent-core/src/harness/mod.rs` — public harness exports.
- `crates/pi-agent-core/tests/harness_errors.rs` — focused filesystem/execution constructor, field, code, cause, chain, and trait proof.
- `crates/pi-agent-core/tests/compaction_errors.rs` — focused compaction/branch-summary constructor, field, code, cause, chain, mutation, and trait proof.
- `oracle/truncate-oracle.ts` — existing pinned utility oracle; compaction/branch-summary differential support is deferred.
- `crates/pi-agent-core/tests/differential_truncate/errors.rs` — 14 direct filesystem/execution constructor fixtures (`13000..=13013`); compaction/branch-summary fixtures are deferred.

## Tests asserting this spec

- `crates/pi-agent-core/tests/harness_errors.rs`
- `crates/pi-agent-core/tests/compaction_errors.rs`
- `crates/pi-agent-core/tests/differential_truncate/errors.rs`
- `crates/pi-agent-core/tests/differential_truncate.rs`

## Known gaps (current cycle)

- [ ] Add direct pinned differential proof for compaction and branch-summary constructors.
- [ ] Prove later filesystem, process, compaction, and branch-summary adapters produce these errors from source-compatible backend branches.

## Out of scope

- Generic JavaScript `Result`, `ok`, `err`, `getOrThrow`, `getOrUndefined`, or `toError(unknown)`.
- Arbitrary JavaScript runtime values, JavaScript property descriptors/enumeration/serialization, stack text, prototype identity, and lone-surrogate strings.
- `Send`/`Sync`, `Clone`, equality, serde, parsing, convenience constructors, and dynamic unknown-error models.
- Filesystem I/O, process execution, cancellation, callbacks, temporary files, shell capture, session storage, compaction orchestration, branch summarization, model calls, and downstream tool integration.
