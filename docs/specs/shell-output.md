# Agent-core shell output

Agent-core shell-output behavior ports `packages/agent/src/harness/utils/shell-output.ts` from the pinned TypeScript source. The bounded slices also expose typed filesystem/execution error prerequisites from `packages/agent/src/harness/types.ts`; shell execution, capture, and production of those errors remain later work.

## What it must do

### Filesystem and execution errors

- [x] Represent exact `FileErrorCode` literals from `types.ts:137-145`.
- [x] Preserve `FileError` constructor order, name, message, code, optional path, and typed cause from `types.ts:148-160`.
- [x] Represent exact `ExecutionErrorCode` literals from `types.ts:163-169`.
- [x] Preserve `ExecutionError` constructor order, name, message, code, and typed cause from `types.ts:172-181`.
- [x] Preserve typed causes through Rust error chaining with `std::error::Error::source`.
- [ ] Compare typed errors directly with the pinned TypeScript constructors.

### Binary-output sanitization

- [x] Preserve ordinary Unicode scalar values in source order without normalization or replacement.
- [x] Preserve tab, LF, and CR.
- [x] Remove every other C0 control from U+0000 through U+001F.
- [x] Remove exactly U+FFF9, U+FFFA, and U+FFFB.
- [x] Preserve DEL, C1 controls, format characters outside the explicit removed range, private-use values, unassigned values, noncharacters, combining characters, variation selectors, and supplementary scalars.
- [x] Preserve duplicates and permit an all-removed input to produce an empty string.

## How it works

- [Shell-output traceability](../porting/traceability/shell-output.md)

## Implementation inventory

- `crates/pi-agent-core/src/harness/types.rs` — typed filesystem and execution errors.
- `crates/pi-agent-core/src/harness/mod.rs` — public harness exports.
- `crates/pi-agent-core/tests/harness_errors.rs` — focused typed-error behavior.
- `crates/pi-agent-core/src/harness/utils/shell_output.rs` — pure binary-output sanitization.
- `crates/pi-agent-core/src/harness/utils/mod.rs` — public utility export.
- `crates/pi-agent-core/tests/sanitize_binary_output.rs` — focused shared-domain behavior.
- `oracle/truncate-oracle.ts` — persistent adapter loading the pinned agent-core sanitizer alongside truncation.
- `crates/pi-agent-core/tests/differential_truncate/sanitize.rs` — exact shared-domain and source-only surrogate proof.

## Tests asserting this spec

- `crates/pi-agent-core/tests/harness_errors.rs`
- `crates/pi-agent-core/tests/sanitize_binary_output.rs`
- `crates/pi-agent-core/tests/differential_truncate.rs`
- `crates/pi-agent-core/tests/differential_truncate/sanitize.rs`

## Known gaps (current cycle)

None for this bounded sanitizer slice.

## Out of scope

- JavaScript lone UTF-16 surrogates. The source implementation retains them; Rust `str` cannot represent them.
- `executeShellWithCapture`, immediate post-sanitization CR removal, byte and line progress accounting, tail buffering, truncation, temporary-file spill, error production, cancellation, and late-callback suppression.
- Filesystem and process implementations that produce these typed errors, `ExecutionEnv`, platform shell discovery, process-tree termination, and Bash tool integration.
- The separate sanitizer in `packages/coding-agent/src/utils/shell.ts`; it requires separate source mapping and proof.
