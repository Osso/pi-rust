# Shared Node readline boundary

One byte-to-line authority for agent-core file reads and coding-agent fd output. [Agent-core trace](../porting/traceability/harness-nodejs-env.md); [find trace](../porting/traceability/find.md).

## What it must do

- [x] Preserve the existing agent-core LineBuffer algorithm: CR/LF/CRLF delimiters, lossy UTF-8 only at complete-line/EOF boundaries, no invented final empty line, binary64 line-limit comparison after a completed line.
- [x] File reads still stop/close at the requested line limit before FIFO writer EOF and abort a pending FIFO read promptly.
- [x] FD output uses the same parser, including bare CR and malformed UTF-8; stdout/stderr content and exact JS trimming remain visible through the tool's existing result/error boundaries.
- [x] Explicit input chunks split within a UTF-8 scalar or CRLF retain the same public byte/line behavior; online consumers can take completed lines without discarding partial bytes or CRLF state.

## How it works

- [Find module mapping](../porting/traceability/find.md).

## Implementation inventory

- `crates/pi-js-compat/src/node_readline.rs`: unchanged shared LineBuffer, moved from agent-core.
- `crates/pi-agent-core/src/harness/env/nodejs/read_lines.rs`: existing file/limit/abort reader; finishes an already-completed line when early limit reached.
- `crates/pi-coding-agent-tools/src/core/tools/find.rs`: existing FD reader consumes bytes through the shared buffer; retains EOF-driven lifetime, not another module's idle-grace capture policy.
- `crates/pi-coding-agent-tools/src/core/tools/grep.rs`: online rg consumer drains completed lines through mem::take only.
- `crates/pi-coding-agent-base/src/utils/child_process.rs`: existing per-chunk UTF-8 pipe reader moved from find for shared find/grep stderr decoding, without a second reader algorithm.

## Tests asserting this spec

- `crates/pi-agent-core/tests/repin_node_lines.rs`: actual frozen limit/line-ending matrix and FIFO early-close/abort boundaries.
- `crates/pi-coding-agent/tests/differential_find_process.rs`: actual frozen FD argv/stream/result/error cases.
- `crates/pi-js-compat/tests/node_readline.rs`: direct chunk-splitting/malformed-byte contract.

## Known gaps (current cycle)

None. Shared-byte2/0, agent-core frozen/FIFO3/0, FD37-case differential GREEN; workspace all-target0 errors/0 warnings and js-compat/agent-core all-target clippy -D warnings pass. Package-wide coding-agent lint debt is tracked by the parent.

## Out of scope

No new line-parser algorithm, capture loop, dependency, timer-window emulation or unrelated agent-core behavior change.
