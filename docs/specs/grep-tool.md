# Grep tool

Match `coding-agent/src/core/tools/grep.ts` at immutable4f9e6f8a4. [Module trace](../porting/traceability/grep.md).

## What it must do

- [x] Preserve binary64 limit/context options. Context is positive original value or zero; effective limit uses JavaScript Math.max(1, original/default100), including NaN propagation.
- [x] Preserve source fractional context loops/index lookups and original numeric labels/notices/details; positive infinity spans the file without unsigned arithmetic overflow.
- [x] Read each contextual file once per execution and reuse its cached content across matches, even when a backend would return different subsequent content.
- [x] Cache failed/empty contextual reads, count match events before optional path validation, retain empty-output versus no-matches distinction, and preserve raw UTF-16/number JSON through the existing lossless parser.
- [x] Stream CR/LF/CRLF/malformed UTF-8 through the shared readline drain API; use JS stderr trim and the existing per-chunk text reader shared with find.
- [x] Pre-abort rejects without work; abort while isDirectory is pending is not replayed into the later listener, and abort during contextual readFile after child close is ignored. Do not introduce find's outer first-winner observer.
- [x] Abort/limit sends default SIGTERM once to the live child only; keep draining stdout/stderr and reaping concurrently. Abort remains observable after stdio EOF or parent exit with inherited stderr, and rejects only at close, before contextual reads. A later abort takes precedence over an earlier limit stop.
- [x] Preserve exact optional flag/empty-glob argv, validate original UTF-16 arguments in Node order before encoding, and retain spawn/missing-tool/stat/signal errors and POSIX path-label rules.
- [x] A path-formatting throw in the async close callback remains uncaught through the shared process authority, never ToolError. Retain the original Node name/message/code/errno/syscall; installed default handler exits1, not a generic Rust panic.
- [ ] Complete whole symbol/branch map and final checkpoint checks.
- [x] Reuse prior Text in both render callbacks, including hidden successful results; incompatible components retain native TypeError. Show error output only, collapse to15 lines with exact hint spacing, and preserve numeric warning truthiness/spelling and raw UTF-16.

## How it works

- [Module map and concrete proofs](../porting/traceability/grep.md).

## Implementation inventory

- `crates/pi-coding-agent/src/core/tools/grep.rs`: input/process handling, cached contextual formatting and render callbacks.

## Tests asserting this spec

- `crates/pi-coding-agent/tests/differential_grep_numbers.rs`:34 actual-frozen numeric/context cases and changing read backend; exact content/details/calls.
- `crates/pi-coding-agent/tests/differential_grep_protocol.rs`:24 frozen byte/path/raw-JSON/counter/read-cache/error/cap cases, including combined match/byte/line notices and astral line clipping.
- `crates/pi-coding-agent/tests/differential_grep_tool.rs`: retained real-rg execution cases.
- `crates/pi-coding-agent/tests/grep_rg.rs`: real child argv and missing-binary behavior.
- `crates/pi-coding-agent/tests/differential_grep_abort_windows.rs`:3 gated frozen pre-registration/post-close/preabort histories.
- `crates/pi-coding-agent/tests/differential_grep_child_abort.rs`:4 actual frozen SIGTERM/close/EOF/parent-exit/limit-stop histories; no descendant signal.
- `crates/pi-coding-agent/tests/differential_grep_render_sequence.rs`:23 actual frozen stateful frames, cross-kind errors, visibility/image/collapse and numeric/raw-text formatting.
- `crates/pi-coding-agent/tests/differential_grep_argv.rs`:21 actual frozen argv/NUL/raw-unit/stat/spawn/signal/path-label cases, including a binary removed or made non-executable after discovery.
- `crates/pi-coding-agent/tests/differential_grep_close_error.rs`: real deleted-cwd child-close rejection, exact original native error identity, unsettled tool and default process exit/stderr boundary.

## Known gaps (current cycle)

- [ ] Remaining raw protocol/process/error/abort/format/renderer branches and current-pin closure.

## Out of scope

Unrelated modules, network/deployment, Node/V8/libuv runtime-oddity emulation and invalid JavaScript runtime input types under approved policy. Valid fractional/nonfinite numbers and arbitrary text are not excluded.
