# Extension examples — batch C

Pinned source: `532f7d12`, `packages/coding-agent/examples/extensions/`.
All eight examples are opt-in native source, included with `#[path]` and registered through an explicit `ExtensionRegistry` factory. No default loading, public core API additions, new dependencies, live providers, or date/header changes.

Rust paths below are under `crates/pi-coding-agent/examples/extensions/`; tests are under `crates/pi-coding-agent/tests/`.

| Pinned source lines / symbols and branches | Rust implementation | Behavioral proof |
|---|---|---|
| notify.ts:11–23 `windowsToastScript` | `notify.rs::windows_toast_script` preserves complete script and argv | `example_notify`: private executable records actual argv |
| notify.ts:25–39 OSC 777 / two OSC 99 writes / Windows process | `notify_with_output`, `spawn_windows_notification` | Same test: exact byte/chunk order visible through buffered terminal writer, stdout failure, missing executable |
| notify.ts:41–55 environment precedence and agent_end | `notify_extension`, `notify_with_output` | Same test: default, Kitty, WT+Kitty precedence, no notifications during registration |
| titlebar-spinner.ts:13–20 frames, base title | `titlebar_spinner.rs::FRAMES`, `read_base_title` | `example_titlebar_spinner`: all frames, wrap, session/no session, live rename and reset, process cwd |
| titlebar-spinner.ts:23–58 timer state, stop/start, agent events/shutdown | `stop_animation`, `start_animation`, `titlebar_spinner_extension` | Same test: virtual 80ms ticks, repeated start resets frame, end/shutdown cancel future ticks |
| working-indicator.ts:24–96 modes, colors, frames, descriptions | `working_indicator.rs::Mode::{indicator,description}` | `example_working_indicator`: exact frames/ANSI colors/intervals, hidden and default modes |
| working-indicator.ts:98–123 initial spinner, apply/status, command parsing | `apply_indicator`, `working_indicator_extension` | Same test: session reapply, current mode, JS whitespace, uppercase, invalid arguments, reset, notification order |
| hidden-thinking-label.ts:24–53 default, label state, session apply, set/reset | `hidden_thinking_label.rs::hidden_thinking_label_extension` | `example_hidden_thinking_label`: JS trim, Unicode text, no-argument reset uses absent UI label, later session applies default |
| timed-confirm.ts:13–44 timeout confirm/select | `timed_confirm.rs::timed_confirm_extension` | `example_timed_confirm`: exact command metadata and dialog options, yes/no/select/cancel/empty selection, errors |
| timed-confirm.ts:47–70 AbortController, timeout, clear timer, outcome branches | `confirm_with_signal` | Same test: success, user cancellation, virtual timeout/cancellation signal, warning vs info and effect order |
| confirm-destructive.ts:9–44 before switch, UI guard, new/resume, user scan | `confirm_destructive.rs::confirm_switch` | `example_confirm_destructive`: UI/no UI, confirm/reject/error, empty session and user followed by assistant still prompts |
| confirm-destructive.ts:46–59 before fork, entry prefix, select/cancel | `confirm_destructive_extension` | Same test: first eight UTF-16 units of valid session IDs, short IDs, chosen/declined/dismissed fork, event results |
| dirty-repo-guard.ts:9–43 exec/status/nonrepo/clean/dirty/UI/choice | `dirty_repo_guard.rs::check_dirty_repo` | `example_dirty_repo_guard`: real private git repo, clean tree, two untracked files, nonrepo, count, accept/decline/dismiss, UI/no UI |
| dirty-repo-guard.ts:45–56 switch/fork action strings and cancellation | `dirty_repo_guard_extension` | Same test: new/resume/fork title strings and runner event results |
| file-trigger.ts:14–41 watch/session start, trim/read/inject/clear/catch/UI | `file_trigger.rs::{file_trigger_extension,file_trigger_with_path,inject_and_clear,watch_error}` | `example_file_trigger`: real private file watcher, blank/Unicode contents, ordered message+triggerTurn and clearing, UI/no UI, send failure leaves contents, missing initial file error |

## Fixture and oracle boundaries

Shared batch-A `tests/support/extension_examples_a.rs` and `oracle/extension-examples-a-oracle.ts` are extended, not copied. Both sides use actual extension loaders, runners, event buses, private in-memory sessions, and captured host actions/UI. Existing batch-A request shapes retain their behavior.

- Timers: Tokio virtual time versus deterministic TS timeout/interval callbacks; no wall-clock sleeps. Dialog fixtures return user outcomes or await the real cancellation token; tests assert timeout options, not countdown rendering (owned by the host UI).
- Notification: capture writer follows batch-A model-status's seam. A private Python `powershell.exe` records argv atomically. No real desktop notification command runs. Empty private PATH proves spawn-failure behavior.
- File trigger: only `/tmp/agent-trigger.txt` is redirected to a private physical path; displayed protocol text stays unchanged. Native/Node watchers are real. Single writes avoid truncate/write event coalescing races. Filesystem/message events have bounded failure timeouts, not sleep-based success conditions.
- Notify/file-trigger use pinned `loadExtensions` (jiti) in the shared oracle to retain actual CommonJS `require` and Node fs binding semantics. Other examples reuse batch-A's factory loader.

## Source discrepancies and domains

Classified **preserve source behavior**, not fixes:

- confirm-destructive's comment says messages since the last assistant response; code scans all user messages. A user message followed by an assistant response still prompts.
- notify ignores Windows process spawn/exit failures (`execFile` has no callback). Differential RED reproduced the extra Rust handler error; implementation now preserves the silent source outcome while reaping children.
- file-trigger assumes the file exists at watch registration, never explicitly closes its watcher, and catches every callback read/send/write error. Missing initial file remains a handler error; delivery failure leaves contents untouched. No recovery or watcher shutdown policy added.
- Filesystem notification multiplicity is OS/event-loop dependent. Fixtures issue one external write per trigger; they do not claim atomicity or deduplication for arbitrary concurrent writers.

Generated session entry IDs are ASCII. Fork prefix slicing uses the existing UTF-16 helper; native UI strings cannot contain an isolated surrogate from an invalid/non-generated entry ID.

## Proof

Each of eight test targets first failed because its embedded module was missing. All eight have targeted GREEN evidence. Logs and revision-scoped proof ledger: `/tmp/pi-rust-ext-examples-c/ledger.md` (numbered `NN-*.log` files alongside it). Fixture correction attempts are retained, not counted as successful proof. Formatter ran before each commit. No clippy/check/workspace suites or direct node/tsx/pi executions.

## Accepted review proof (2026-10-20)

F3/F4 public-entrypoint regressions: notify invokes notify_extension with a private Windows executable and verifies its argv receipt. File-trigger invokes file_trigger_extension for all real watcher cases; a cfg(test)-only root maps its literal path into a private temporary tree. No test watches the shared /tmp/agent-trigger.txt. Empty public wrappers fail by bounded missing-side-effect timeouts.
