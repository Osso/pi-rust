# terminal.ts frozen traceability

Reference: immutable `4f9e6f8a4`, `packages/tui/src/terminal.ts` (567 lines).
Rust: `crates/pi-tui/src/terminal.rs`. [Q1 proof](../q1-tui-core-proof.md).

| Export/function group | Rust mapping and exact behavior | Proof |
|---|---|---|
| `KeyboardProtocolNegotiationSequence`, `parseKeyboardProtocolNegotiationSequence`, prefix check | f64 finite flags, anchored decimal Kitty/DA grammar, partial-prefix buffering | unit + differential, including flags beyond u32/u64 |
| `isAppleTerminalSession`, `normalizeAppleTerminalInput` | macOS + Apple_Terminal only; bare CR plus native Shift becomes CSI-u Shift+Enter | unit + differential |
| `TerminalRawModeError`, cause formatting, `isTerminalRawModeFailure` | typed enable/restore messages and case-insensitive Bun errno prefix | unit + differential |
| `Terminal`, `ProcessTerminal`, getters | event-queue/poll adaptation; dimensions use terminal size then env/default; getter flags exposed | unit, real PTYs/pipes |
| constructor/write-log initialization | configured file or local `tui-YYYY-MM-DD_HH-MM-SS-<pid>.log` in configured directory | real frozen write differential in America/Chicago; timestamp checked against invocation interval |
| `start`, raw-mode switch, resize setup, `setupStdinBuffer` | raw mode only for TTY stdin; bracketed paste; fixed 10ms buffer; worker with stateful UTF-8 decoding | real frozen PTY/pipe differential |
| query/push, read/set/clear/flush negotiation buffer and timer | request flags 7, query flags and DA; 150ms fragment reassembly; nonzero Kitty/zero-or-DA modifyOtherKeys; forward unrelated input | unit + real fragmented negotiation |
| `forwardInputSequence`, paste callback | Apple-only normalization, paste rewrapped with bracket markers | differential helpers + real ordered input/paste events |
| enable/disable modifyOtherKeys | exact control bytes, idempotence guards | unit + real DA/Kitty/late-response cases |
| `enableWindowsVTInput` | not implemented on Linux | approved native gap; [native classification](native-modifiers.md) |
| `drainInput` | disables protocol first, suppresses forwarded input, extends idle window on **raw chunks**, including consumed negotiation replies | real frozen late-response idle-boundary differential |
| `stop` | progress clear, paste disable, protocol pop/reset, worker cancellation/join, raw restoration | exact full-stream bytes and exact termios restoration across two lifetimes |
| `write` + write-log failure | output bytes, append payload; logging error intentionally ignored without creating missing parents | unit I/O-error queue + real frozen logging side-effect differential |
| moveBy/hide/show/clearLine/clearFromCursor/clearScreen/setTitle | source escape sequences | capturing-terminal/core render tests |
| progress interval/keepalive and clear | source active OSC and clear `OSC 9;4;0; BEL`; polling drives keepalive | unit + real start/drain/stop output |

## Re-pin changes

Removed later `resolveEscapeTimeoutMs`/SSH override API and `normalizeNativeShiftEnterInput` export. Preserved finite large flags and long finite colors. Write-log local date/pid and no-mkdir error behavior now match frozen.

The real-stream `terminal_stream` target runs frozen ProcessTerminal and Rust as separate real stdin/stdout processes under Python-created private PTYs in the mandatory sandbox. It reproduces split UTF-8 across OS reads, fragmented negotiation, DA fallback, repeated terminal lifetimes, redirected pipes, resize, paste, late raw negotiation during drain and termios restoration. REDs found lossy per-read decoding, an old reader stealing the next lifetime's response, ENOTTY startup and premature idle exit; all fixed at those boundaries.

The driver waits for DRAINING before injecting late data (300ms idle, 1000ms max). The old 20ms wall-clock injection could run after frozen restored raw mode under concurrent compilation; the handshake fixes the test, not production state.

## Representation and pending classification

Callbacks/Promise timers become event queues and synchronous polling. Resize and progress are polled; original termios is restored. Stream I/O failures use `TerminalIoError`/`take_errors`; the Rust crash-restore handle is a native runtime detail, not a frozen package-root export.

Frozen lines 501-506 return negative/fractional/±Infinity env dimensions; Rust usize cannot. Frozen line 28 permits Infinity flags from 400 decimal digits; Rust currently rejects non-finite flags. Parent deferred these numeric domains to user classification; coverage remains **partial**, not done. Exact captures: `/tmp/pi-rust-repin-tui-core/numeric-domains.log`.
