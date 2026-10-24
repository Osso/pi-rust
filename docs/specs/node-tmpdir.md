# POSIX Node temporary directory

Port Node v26.11.1 `os.tmpdir()` for frozen `node:os` consumers. Reference: `lib/os.js::tmpdir()` and its POSIX `getTempDir` binding. [Authority and proof map](../porting/traceability/node-os.md).

## What it must do

- [x] Read the shared process environment with priority `TMPDIR || TMP || TEMP || '/tmp'`; empty values do not win.
- [x] Strip exactly one final slash from non-root values, preserving `/`, relative paths, backslashes and Unicode text.
- [x] Executor and output-accumulator spill paths use this authority and observe environment mutations; accumulator defaults are queried at first spill and remain fixed after close.
- [x] The two additionally assigned consumers query this authority: harness temporary directories/files (frozen `harness/env/nodejs.ts:548`) and wrapper full-output paths (frozen `tool-definition-wrapper.ts:21`).
- [x] Assigned interactive/clipboard consumers query the same authority at use time: frozen `clipboard-image.ts:162` PowerShell staging, `interactive-mode.ts:3842` clipboard-image staging, and `:7328` share export. The inline external editor `:5455` uses it in its [literal lifecycle](external-editor.md).

## How it works

- [Shared Node OS authority](../porting/traceability/node-os.md).

## Implementation inventory

- `crates/pi-js-compat/src/node_os.rs`: shared POSIX query.
- `crates/pi-coding-agent/src/core/bash_executor.rs`: executor spill-path consumer.
- `crates/pi-coding-agent/src/core/tools/output_accumulator.rs`: accumulator spill-path consumer.
- `crates/pi-agent-core/src/harness/env/nodejs.rs`: temporary-directory/file base.
- `crates/pi-coding-agent/src/core/tools/tool_definition_wrapper.rs`: full-output spill base.

## Tests asserting this spec

- `crates/pi-js-compat/tests/node_tmpdir.rs`: eleven actual Node-v26.11.1 comparisons, including `/`, `//`, `///`, relative, Unicode and backslash values.
- `crates/pi-coding-agent/tests/differential_bash_executor_spill.rs`: real spill paths under mutable environment priorities.
- `crates/pi-coding-agent/tests/differential_tmpdir_consumers.rs`: first-spill/closed-file accumulator lifetime and assigned harness/wrapper consumers.
- `interactive_mode::ui_tests::tmpdir_clipboard_and_share_consumers_match_frozen_mutable_environment`: three actual source/native consumers × four mutable environment cases. Fake tool/gh processes prevent live clipboard/network access. The source share dependency captures the export-path argument before deliberately rejecting; native uses a persisted local session and fake gh.

## Known gaps (current cycle)

None for this cleared POSIX getter/consumer scope: helper11, executor17 and twelve accumulator/harness/wrapper cases pass. Whole accumulator/bash module audits remain separate.

## Out of scope

Windows temp-directory semantics; this authority is explicitly POSIX. This consumer extension changes temporary roots only, not clipboard naming/tool failure behavior or the share lifecycle; their module audits remain separate.
