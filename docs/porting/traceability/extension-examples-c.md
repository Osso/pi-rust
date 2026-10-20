# Extension examples — batch C

Pinned source: `532f7d12`, `packages/coding-agent/examples/extensions/`.
Native embedding only: each source is included by its test via `#[path]` and registered in an explicit ExtensionRegistry factory. No default loading or public core API additions.

| Source lines | Rust example / behavior | Runtime differential test |
|---|---|---|
| working-indicator.ts:24–123 | `working_indicator.rs`: rainbow frames, intervals, current mode, reset, invalid arguments, status and notification order | `example_working_indicator::modes_frames_order_reset_and_invalid_input_match_ts` |
| hidden-thinking-label.ts:24–53 | `hidden_thinking_label.rs`: default, trim, set/reset, next session reapply | `example_hidden_thinking_label::label_reset_and_subsequent_session_start_match_ts` |
| confirm-destructive.ts:9–59 | `confirm_destructive.rs`: UI guard, new/resume and user-entry scan, fork prefix/choice, cancellation, dialog errors | `example_confirm_destructive::switches_forks_ui_gating_and_dialog_errors_match_ts` |
| dirty-repo-guard.ts:9–56 | `dirty_repo_guard.rs`: actual git execution, non-repo/clean/dirty, file count, UI/no-UI, switch/fork decisions | `example_dirty_repo_guard::actual_private_git_clean_dirty_and_non_repository_match_ts` |

Proof: RED missing-module compilation observed for each test; GREEN tracked in `/tmp/pi-rust-ext-examples-c/ledger.md`. Shared batch-A fixture/oracle extended, not copied. Both sides load unchanged examples through real extension runtimes; only UI/actions are fixtures. Git uses a private temporary directory, never host repositories.

Source discrepancy: confirm-destructive's comment describes messages since the last assistant response, but its implementation scans **all** user messages. Rust preserves implementation.

Typed Rust entry IDs are native strings; the existing UTF-16 slicing helper applies to fork title prefixes. Real generated session entry IDs are ASCII.

| timed-confirm.ts:13–70 | `timed_confirm.rs`: timeout options, select truthiness, manual cancellation token, user/timeout/cancel outcomes, UI errors | `example_timed_confirm::timed_dialog_options_results_abort_and_errors_match_ts` |
| titlebar-spinner.ts:13–58 | `titlebar_spinner.rs`: live session name, process cwd basename, all ten frames, wrap, repeated start/end/shutdown | `example_titlebar_spinner::deterministic_frames_wrap_restart_and_shutdown_match_ts` |

Timers use Tokio virtual time; the shared TS oracle drives equivalent virtual interval/timeout callbacks. No wall-clock sleeps. Remaining examples: notify, file-trigger.
