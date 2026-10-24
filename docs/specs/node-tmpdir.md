# POSIX Node temporary directory

Port Node v26.11.1 `os.tmpdir()` for frozen `node:os` consumers. Reference: `lib/os.js::tmpdir()` and its POSIX `getTempDir` binding. [Authority and proof map](../porting/traceability/node-os.md).

## What it must do

- [ ] Read the shared process environment with priority `TMPDIR || TMP || TEMP || '/tmp'`; empty values do not win.
- [ ] Strip exactly one final slash from non-root values, preserving `/`, relative paths, backslashes and Unicode text.
- [ ] Executor and output-accumulator spill paths use this authority and observe environment mutations.

## How it works

- [Shared Node OS authority](../porting/traceability/node-os.md).

## Implementation inventory

- `crates/pi-js-compat/src/node_os.rs`: shared POSIX query.
- `crates/pi-coding-agent/src/core/bash_executor.rs`: executor spill-path consumer.
- `crates/pi-coding-agent/src/core/tools/output_accumulator.rs`: accumulator spill-path consumer.

## Tests asserting this spec

- `crates/pi-js-compat/tests/node_tmpdir.rs`: eleven actual Node-v26.11.1 comparisons, including `/`, `//`, `///`, relative, Unicode and backslash values.
- `crates/pi-coding-agent/tests/differential_bash_executor_spill.rs`: real spill paths under mutable environment priorities.

## Known gaps (current cycle)

- [ ] New helper and routed-consumer verification pending.

## Out of scope

Windows temp-directory semantics; this authority is explicitly POSIX. Other production consumers are assigned separately rather than silently migrated.
