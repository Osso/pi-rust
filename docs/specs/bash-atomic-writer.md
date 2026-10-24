# Detached Bash atomic JSON writer

Frozen `packages/coding-agent/src/core/detached-bash-runner.ts:270–295` writes JSON artifact records atomically. This contract covers the cleared writer prerequisite, not the whole detached runner. [Implementation/proofs](../porting/traceability/detached-bash-runner.md).

## What it must do

- [x] Write shared JS JSON bytes plus one LF to `<path>.tmp`, creating with mode0600 and truncating existing contents; close the write descriptor before proceeding.
- [x] Reopen the temporary file read-only, fsync it and close in finally; then rename to the final path and independently open/fsync/finally-close its directory.
- [x] Preserve Node open/write/fsync/rename/close error messages and temporary/final file states at each failure boundary. A finally-close error overrides an earlier write/fsync error.
- [x] Real Bash activation surfaces the writer's original message at the high-level rejection boundary, without reporting activation success.

## How it works

- [Atomic writer proof matrix](../porting/traceability/detached-bash-runner.md#current-pin-atomic-writer-prerequisite).
- [Shared Node filesystem diagnostics](../porting/traceability/node-fs-errors.md).

## Implementation inventory

- `crates/pi-coding-agent/src/core/detached_bash_runner.rs`: write/close, read-open/fsync/close, rename and directory-sync sequence.
- `crates/pi-js-compat/src/node_fs.rs`: existing Node diagnostics; fd-only fsync omits a pathname like fd-only write/close.

## Tests asserting this spec

- `crates/pi-coding-agent/tests/differential_bash_atomic_writer.rs`: twelve actual frozen/native histories, real syscall faults, complete file bytes/modes/states and close-error precedence.
- `crates/pi-coding-agent/tests/bin/bash_runner_lifecycle.rs`: real process activation rejection plus twenty retained foreground/detach/completion histories.
- `crates/pi-coding-agent/tests/differential_bash_manifest_numbers.rs`: complete launch-file bytes and checksums.

## Known gaps (current cycle)

None in the cleared writer scope; [checkpoint evidence](../porting/traceability/detached-bash-runner.md#current-pin-atomic-writer-prerequisite) records passing focused proofs and the unrelated package-global lint blocker.

## Out of scope

External manifest schema reconciliation and other whole-runner branches remain assigned to the parent. No new writer queue, fallback or production fault-injection seam.
