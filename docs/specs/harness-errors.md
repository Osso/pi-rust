# Agent-core harness errors

Agent-core exposes the typed filesystem and process-execution errors required by the harness capability contracts. The pinned source definitions are `packages/agent/src/harness/types.ts:137-181`; effectful filesystem and process implementations remain later slices.

## What it must do

### Error codes and exports

- [x] Expose every `FileErrorCode` literal through a closed Rust enum and exact `as_str()` mapping.
- [x] Expose every `ExecutionErrorCode` literal through a closed Rust enum and exact `as_str()` mapping.
- [x] Export all four Rust types through `pi_agent_core::harness` and invoke both TypeScript classes through the public agent package index.

### Error values

- [x] Preserve `FileError` public `name`, `message`, `code`, optional `path`, and optional typed `cause` fields.
- [x] Preserve `ExecutionError` public `name`, `message`, `code`, and optional typed `cause` fields.
- [x] Preserve source constructor ordering and exact initialized names.
- [x] Reflect current public message and cause fields through `Display` and `Error::source()`.
- [x] Preserve empty, Unicode, supplementary, and NUL-containing Rust strings.
- [x] Compare 14 normalized constructor values directly with the pinned TypeScript classes through `harness_error` requests `13000..=13013` in `oracle/truncate-oracle.ts`.

## How it works

- [Harness-error traceability](../porting/traceability/harness-errors.md)

## Implementation inventory

- `crates/pi-agent-core/src/harness/types.rs` — typed error codes and error values.
- `crates/pi-agent-core/src/harness/mod.rs` — public harness exports.
- `crates/pi-agent-core/tests/harness_errors.rs` — focused constructor, field, code, cause, chain, and trait proof.
- `oracle/truncate-oracle.ts` — loads the public pinned agent package module and handles recursive `harness_error` normalization.
- `crates/pi-agent-core/tests/differential_truncate/errors.rs` — 14 direct constructor fixtures (`13000..=13013`).

## Tests asserting this spec

- `crates/pi-agent-core/tests/harness_errors.rs`
- `crates/pi-agent-core/tests/differential_truncate/errors.rs`
- `crates/pi-agent-core/tests/differential_truncate.rs`

## Known gaps (current cycle)

- [ ] Prove later filesystem and process adapters produce these errors from source-compatible backend branches.

## Out of scope

- Generic JavaScript `Result`, `ok`, `err`, `getOrThrow`, `getOrUndefined`, or `toError(unknown)`.
- Arbitrary JavaScript runtime values, JavaScript property descriptors/enumeration/serialization, stack text, prototype identity, and lone-surrogate strings.
- `Send`/`Sync`, `Clone`, equality, serde, parsing, convenience constructors, and dynamic unknown-error models.
- Filesystem I/O, process execution, cancellation, callbacks, temporary files, shell capture, session storage, and downstream tool integration.
