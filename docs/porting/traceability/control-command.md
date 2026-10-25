# Coding-agent control command traceability

Frozen: `packages/coding-agent/src/cli/control-command.ts` at `4f9e6f8a4c0065d81b10e7177b28d5082f0bc30d`. All 128 lines compared with `crates/pi-coding-agent/src/cli/control_command.rs`.

| Export / branches | Proof |
| --- | --- |
| `handleControlCommand`: non-control ignored; help/unknown output destination and exitCode | 17-case actual-source `suite_core_1::control_command::control_help_and_argument_rejections_match_frozen_without_side_effects`; exact complete writes and no DB creation |
| `send`: help takes precedence over invalid pid/message, stdout help/exit0, no queue/signal | Same differential; RED `/tmp/settings-models-control-red.log` shows old Rust queued `--help` as a message |
| `parseSendCommand`: iterate args, Number conversion/integer/positive validation, last pid wins, remaining words join and JS trim; empty rejected | Actual-source invalid/empty/missing/decimal pid cases; native enqueue/signaled pid/content fixture |
| `send`: enqueue precedes stdout, then optional signal and second stdout; errors propagate | Native real SQLite `queues_a_control_message_and_signals_a_running_process`; actual frozen default signal failure against a reaped child |
| `restart`: exact flag/count/trim, health missing/dead rejection; healthy SIGHUP and success line; signal failure catch/exit1 | Native health DB fixtures and malformed-restart actual-source differential; explicit signal-failure behavior test |
| `last`: optional last record, exact content plus newline | Native real DB `prints_the_last_assistant_reply`; empty branch has no output by inspection |
| `path`: state-root DB path plus newline | Native injected-state-root fixture |
| `printControlHelp`: exact multiline output | All 17 differential scenarios compare full writes |
| Default `process.kill`: syscall error text | `signaling_an_exited_process_preserves_node_error_text`: actual frozen send against reaped child returns `kill ESRCH`; RED `/tmp/settings-models-control-signal-red.log` showed Rust OS prose |

The default Rust signal effect maps errno through the shared Node code authority, producing `kill <code>`. Known runtime/fault/numeric-extreme shapes remain outside the current parity bar; no additional carrier is introduced for out-of-range JavaScript pids. POSIX signal behavior is covered; Windows remains the approved platform gap.

Post-commit GREEN, warning-mode clippy and changed-line readability results are recorded in `/tmp/settings-models-proof-ledger.md`.
