# Grep tool

Match `coding-agent/src/core/tools/grep.ts` at immutable4f9e6f8a4. [Module trace](../porting/traceability/grep.md).

## What it must do

- [x] Preserve binary64 limit/context options. Context is positive original value or zero; effective limit uses JavaScript Math.max(1, original/default100), including NaN propagation.
- [x] Preserve source fractional context loops/index lookups and original numeric labels/notices/details; positive infinity spans the file without unsigned arithmetic overflow.
- [x] Read each contextual file once per execution and reuse its cached content across matches, even when a backend would return different subsequent content.
- [ ] Prove failed/empty contextual file caching and complete raw rg JSON/IO/lifetime/cancellation/error semantics.
- [ ] Complete renderer state/visibility/number semantics and whole symbol/branch map.

## How it works

- [Module map and concrete proofs](../porting/traceability/grep.md).

## Implementation inventory

- `crates/pi-coding-agent/src/core/tools/grep.rs`: input/process handling, cached contextual formatting and render callbacks.

## Tests asserting this spec

- `crates/pi-coding-agent/tests/differential_grep_numbers.rs`:34 actual-frozen numeric/context cases and changing read backend; exact content/details/calls.
- `crates/pi-coding-agent/tests/differential_grep_tool.rs`: retained real-rg execution cases.
- `crates/pi-coding-agent/tests/grep_rg.rs`: real child argv and missing-binary behavior.

## Known gaps (current cycle)

- [ ] Remaining raw protocol/process/error/abort/format/renderer branches and current-pin closure.

## Out of scope

Unrelated modules, network/deployment, Node/V8/libuv runtime-oddity emulation and invalid JavaScript runtime input types under approved policy. Valid fractional/nonfinite numbers and arbitrary text are not excluded.
