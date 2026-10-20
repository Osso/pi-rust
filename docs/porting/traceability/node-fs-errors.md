# Shared Node filesystem errors

Rust: `crates/pi-js-compat/src/node_fs.rs`. Pure compatibility prerequisite for Node builtin filesystem errors, not a new TypeScript application module. Node reference: `oracle/node-fs-error-oracle.ts`, executed through existing `TsOracle` source-lock verification, tsx and parent watchdog. All filesystem fixtures are isolated temporary paths; no providers or network.

| Public API / branch | Behavioral proof |
|---|---|
| `NodeFsError`, `node_fs_error_record`: Error name, message, negative Unix errno, code, syscall, optional path | `pi-js-compat/tests/node_fs_errors.rs::real_filesystem_messages_and_own_metadata_match_node`: 8 actual filesystem cases, exact message and complete own metadata (stack excluded from comparison) |
| `node_fs_error`: message projection | Same corpus: missing-parent write/read, directory write/read, ENOTDIR write/read, EEXIST mkdir, symlink-loop read |
| `node_read_error`: read syscall for EISDIR, open otherwise | Same corpus plus coding-agent `tests/node_fs_errors.rs` (3 tests at preserved public paths) |
| `node_error_code`, `node_errno_name`: existing raw errno-first table and ErrorKind mapping moved unchanged | Shared constructed-kind and raw-code tests; coding-agent `tests/errno_codes.rs` (6 existing real exec-boundary tests) |
| ErrorKind-only construction | 8 kinds retain existing messages/code/path; errno stays absent, including directory-read legacy path |
| Unproven descriptions | ETXTBSY retains std text; no full libuv strerror expansion |

## Classified conformance fixes

Real Node RED before production changes proved two Rust compatibility bugs: EISDIR directory reads incorrectly appended a path; ELOOP reads emitted std OS wording instead of `too many symbolic links encountered`. Raw EISDIR/read now has no path metadata and no path suffix. EISDIR/open retains path. ELOOP uses the proven Node description. No pinned TS source bug changed.

`core::tools::node_fs::{node_fs_error,node_read_error}` and `utils::child_process::{node_errno_name,node_error_code}` re-export existing signatures. The model-data private adapter wraps the shared message in `JsString`; reads select `read` for directory errors. There is no coding-agent dependency from pi-ai. Existing model-data ELOOP and directory-read behavior is retained.

## Explicit gaps

- Exact negative errno and filesystem differential proof are Linux/Unix bounded; Windows libuv errno translation is not asserted.
- Unknown errno descriptions retain the existing std text rather than claiming complete Node conformance.
- Existing `node_errno_name(0)` panics in rustix; zero is not a filesystem failure errno. Preserved outside this observed-error slice, not tested as supported behavior.
- No stack is fabricated or removed by this helper. The record contains no stack; generator-specific stack presentation belongs to its caller.
- Typed Rust inputs exclude invalid JS argument types. Paths here are UTF-8 strings; arbitrary non-UTF-8 OS path parity is not established.

## Focused proof

Ledger: `/tmp/pi-rust-shared-node-fs-proof`; RED artifacts `-red2.log` (1 pass/1 fail), `-red3.log` (1 pass/2 fail). Shared metadata corpus passed on first implementation; an invalid zero-errno test exposed the inherited gap above and was removed without changing production. Final focused command results and revision are recorded in ledger. No check, clippy, readability, coverage, broad suite or final gates run; integration belongs to parent.
