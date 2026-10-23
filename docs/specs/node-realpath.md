# Node realpath boundary

Shared filesystem authority for the frozen coding-agent tool callers. [Proof map](../porting/traceability/node-fs-errors.md).

## Contract
- [x] Async realpath validates the original UTF-16 path before filesystem IO, using the same string-validation authority as child_process.
- [x] Reject NUL with native TypeError, ERR_INVALID_ARG_VALUE, exact inspected message and coded String(error); do not fabricate errno/syscall/path metadata.
- [x] Encode valid paths only at the OS boundary; decode returned path bytes with Node UTF-8 replacement.
- [x] Preserve OS failures as native Error data: message, negative errno, code, realpath syscall and original API-specific path projection.
- [x] Serialize mutation-queue key resolution and registration in source call order. Only ENOENT/ENOTDIR use the frozen resolved-path key.
- [x] Read config-root checks share the async authority. ask_architect shares synchronous native lookup; its frozen catch replaces lookup failures with the fixed tool message.

## Boundaries
`realpath_sync_native` models realpathSync.native, not the default JavaScript realpathSync component-by-component lstat walk. The ask_architect caller consumes successful canonical paths and catches every lookup failure. Default sync-walk diagnostics are not claimed by this helper.

Typed string inputs only; Windows filesystem behavior remains the recorded platform gap.

## Proof
- `pi-coding-agent/tests/node_realpath.rs`: nine paths against both actual Node bindings, including aliases, invalid OS bytes, missing/ENOTDIR/ELOOP and raw/long NUL diagnostics; constructor, String(error), values and metadata exact.
- `differential_write_filesystem`: sixteen actual frozen write requests, final contents, modes and root entries.
- `differential_write_queue`: eight actual frozen gated-operation scenarios; a later independent native registration proves the same-file waiter passed async key resolution before releasing the active operation.
- `file_mutation_queue`, `differential_read_config_root`, `lossless_local_path_validation`: queue and shared-validation regressions.
