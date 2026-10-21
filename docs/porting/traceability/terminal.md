# terminal.ts traceability

Source: `packages/tui/src/terminal.ts` (594 lines, pinned revision in
`docs/porting/source-lock.json`). Rust: `crates/pi-tui/src/terminal.rs`.

## Exported symbols

| TS symbol | Rust item | Tests |
| --- | --- | --- |
| `KeyboardProtocolNegotiationSequence` | `KeyboardProtocolNegotiationSequence` | `parses_keyboard_protocol_negotiation_sequences` |
| `parseKeyboardProtocolNegotiationSequence` | `parse_keyboard_protocol_negotiation_sequence` | unit + differential |
| `isAppleTerminalSession` | `is_apple_terminal_session(platform, term_program)` | `detects_apple_terminal_only_on_macos_with_the_apple_program` |
| `normalizeNativeShiftEnterInput` | `normalize_native_shift_enter_input` | 4 unit tests + differential |
| `TerminalRawModeError` | `TerminalRawModeError` (`Display`, `Error`) | `converts_raw_mode_failures_into_typed_errors` |
| `isTerminalRawModeFailure` | `is_terminal_raw_mode_failure` | typed + message-prefix tests + differential |
| `normalizeAppleTerminalInput` | `normalize_apple_terminal_input` | 4 unit tests + differential |
| `Terminal` | `Terminal` trait + `TerminalEvent` | every TUI test uses a `VirtualTerminal` implementation |
| `resolveEscapeTimeoutMs` | `resolve_escape_timeout_ms` | 5 unit tests + differential |
| `ProcessTerminal` | `ProcessTerminal`; Rust-native `Terminal::crash_restore` / `TerminalRestore::{publish_tui, restore_then}` publish saved termios and stop bytes across threads | negotiation/progress/dimension tests; coding-agent `interactive_crash::worker_restore_matches_normal_ui_stop_bytes` (real PTY, regular/fullscreen, progress, color notifications, Kitty/modifyOtherKeys, exact bytes and termios) |

## Branches

| Source branch | Rust item | Tests |
| --- | --- | --- |
| kitty-flags response regex / device-attributes regex | `parse_keyboard_protocol_negotiation_sequence` | parse test + differential |
| negotiation prefix (`ESC [`, `ESC [ ? [\d;]*`) | `is_keyboard_protocol_negotiation_sequence_prefix` | split-CSI replay test |
| `PI_TUI_ESC_TIMEOUT` finite/positive override | `resolve_escape_timeout_ms` (`Number(string)` via `pi_js_compat::number::parse_number`) | override/invalid tests, `environment_numbers_use_javascript_number_parsing` |
| SSH defaults (100 ms) vs 10 ms | `resolve_escape_timeout_ms` | SSH/default tests |
| native Shift+Enter rewrite only for `\r` + shift | `normalize_native_shift_enter_input` | rewrite/no-op tests |
| raw-mode error message by flag | `TerminalRawModeError::Display` | typed-error test |
| Bun `setRawMode failed with errno:` message detection | `is_terminal_raw_mode_failure` | message test |
| start: bracketed paste, raw mode, query, stdin reader | `ProcessTerminal::start` | negotiation harness; differential TUI start writes |
| negotiation buffer: append, prefix pending, replay | `read_keyboard_protocol_negotiation_sequence`, `flush_*` | split confirmation + CSI replay |
| 150 ms fragment flush timer | `keyboard_protocol_flush_deadline` via `poll_at` | CSI replay test |
| kitty flags non-zero → activate, zero → modifyOtherKeys | `handle_keyboard_protocol_negotiation_sequence` | three fallback tests |
| device attributes → modifyOtherKeys when kitty inactive | same | DA test |
| normal input forwarded while negotiating | `forward_input_sequence` | forward test |
| paste re-wrapped as `ESC[200~…ESC[201~` | `dispatch_stdin_events` | exercised through `StdinBuffer`; no dedicated terminal test (stdin-buffer slice owns paste parsing) |
| drainInput disables kitty/modifyOtherKeys and waits | `drain_input` | gap: real stdin timing |
| stop: progress clear, paste off, kitty disable, buffer destroy, raw restore | `ProcessTerminal::stop` | kitty/modifyOtherKeys stop assertions |
| write log path (dir → timestamped file, else as-is) | `write_log_path`/`append_write_log` | gap: debug artifact only |
| columns/rows fallbacks | `resolve_columns`/`resolve_rows` (`Number(string)`) | dimension test, `environment_numbers_use_javascript_number_parsing` |
| progress OSC 9;4 active/clear + 1 s keepalive | `set_progress`/`refresh_progress_keepalive` | clear-sequence test |
| write log errors ignored | `append_write_log` | gap: I/O failure injection |

## Differential

`crates/pi-tui/tests/differential_terminal.rs` runs
`oracle/tui-core-oracle.ts` over ten environments and compares
`resolveEscapeTimeoutMs`, six negotiation sequences, six normalization cases, and
three raw-mode failure cases exactly.

## Deviations and gaps

- `Terminal::start` takes no callbacks. Events are pulled with
  `Terminal::poll_events` and `TerminalEvent::{Input,Resize}`. This avoids
  re-entrant `RefCell` borrows and lets `ProcessTerminal` own a stdin reader
  thread that cannot call back into non-`Send` TUI state.
- `drainInput` is synchronous (`fn drain_input(max_ms, idle_ms)`) instead of
  returning a JS `Promise`.
- `start` returns `Result<(), TerminalRawModeError>` instead of throwing. The Bun
  `setRawMode` error-event path cannot be injected through a Rust `stdin`; the
  typed error and message detection are tested directly.
- `ProcessTerminal` stores the original `Termios` and restores it on stop instead
  of `setRawMode(wasRaw)`. Same observable raw-mode restoration, stronger state.
- Resize detection polls `tcgetwinsize` in `poll_events` instead of installing a
  SIGWINCH handler (signal handlers require unsafe).
- Progress keepalive is checked in `poll_events` instead of a `setInterval`.
- `PI_TUI_WRITE_LOG` directory filenames use `tui-<unix-seconds>-<pid>.log`
  instead of a local `YYYY-MM-DD_HH-MM-SS` timestamp.
- `PI_TUI_ESC_TIMEOUT`/`COLUMNS`/`LINES` use `pi_js_compat::number::parse_number`, so hex/binary/octal literals and `Infinity` match JS `Number`.

## Q1 re-pin audit (2026-10-21)

Reference: immutable `4f9e6f8a4`. Removed later escape-timeout env/SSH helper and native Shift normalization export. ProcessTerminal uses fixed 10ms stdin buffering and Apple-only Shift+Enter. Progress clear is the frozen OSC 9;4;0;BEL (extra semicolon); capture-output differential RED/GREEN compares actual setProgress calls without touching a live terminal. Existing negotiation event tests cover split input, zero/nonzero flags, DA fallback and stop.

Finite Kitty flag numbers beyond u32/u64 are represented as f64 and match parseInt output; flags 4294967296 and 18446744073709551616 have differential RED proof. Non-finite flag reports and negative/fractional/non-finite environment dimensions remain explicit numeric-domain gaps pending user classification.

Q1 parent-directed write-log closure: directory filenames now use frozen local YYYY-MM-DD_HH-MM-SS plus pid, not Unix seconds. `differential_terminal_write_log` compares a real frozen ProcessTerminal write, validates the local timestamp against the invocation interval in America/Chicago, and checks exact payload. Failed logging never creates missing parent directories, matching the frozen catch-and-ignore branch. RED filename `/tmp/q1-write-log-red.log`, RED side effect `/tmp/q1-write-log-parent-red.log`.

## Real-stream Q1 proof

`terminal_stream` is a harness-free integration target driven by Python-created private PTYs inside the mandatory runner. It runs the unmodified frozen ProcessTerminal and the Rust terminal as separate real stdin/stdout processes. Matrix: fragmented Kitty negotiation plus UTF-8 split across OS writes; DA fallback through two start/drain/stop lifetimes; redirected non-TTY pipes. Each compares complete output bytes, ordered input/paste events, resize dimensions, active negotiation state, exit status and exact termios restoration. Late input during drain is not forwarded.

RED logs `/tmp/q1-stream-red2.log`, `/tmp/q1-stream-restart-red.log`, `/tmp/q1-stream-pipe-red.log` prove three actual boundaries: lossy per-read UTF-8 conversion; stopped reader stealing the next lifecycle negotiation response; ENOTTY raw-mode failure where frozen skips absent setRawMode. Fixes retain incomplete UTF-8 across reads; poll stdin with a bounded cancellation deadline and join the old reader before stop returns; switch termios only for real TTY stdin. No host terminal is accessed.

Real drain timing also matches raw stdin activity, including late negotiation responses that produce no input event. Direct RED `/tmp/q1-stream-drain-red.log` shows Rust stopping before the idle extension while frozen waits; drain now updates idle time from received chunks before evaluating the idle boundary.

The PTY driver synchronizes on an explicit DRAINING marker before injecting the late response, using a 300ms idle window with a 1000ms maximum. This avoids relying on a 20ms parent scheduling delay under concurrent compilation; frozen output had demonstrated the old fixture could inject after raw-mode restoration. The source and Rust production idle semantics are unchanged by this fixture repair.
