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
| `resolveEscapeTimeoutMs` | `resolve_escape_timeout_ms` | 4 unit tests + differential |
| `ProcessTerminal` | `ProcessTerminal` | negotiation/progress/dimension tests |

## Branches

| Source branch | Rust item | Tests |
| --- | --- | --- |
| kitty-flags response regex / device-attributes regex | `parse_keyboard_protocol_negotiation_sequence` | parse test + differential |
| negotiation prefix (`ESC [`, `ESC [ ? [\d;]*`) | `is_keyboard_protocol_negotiation_sequence_prefix` | split-CSI replay test |
| `PI_TUI_ESC_TIMEOUT` finite/positive override | `resolve_escape_timeout_ms` | override/invalid tests |
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
| columns/rows fallbacks | `resolve_columns`/`resolve_rows` | dimension test |
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
- Number parsing for `PI_TUI_ESC_TIMEOUT`/`COLUMNS`/`LINES` covers decimal and
  exponent forms; JS `Number` hex/Infinity edge cases are not reproduced.
