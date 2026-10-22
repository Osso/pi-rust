# session-picker traceability

Source: `packages/coding-agent/src/cli/session-picker.ts` at authorized `c248b0067`
(now included in [common source lock](../source-lock.json); 2026-10-20 approval to re-validate against 4f9e6f8a4).
Rust: `crates/pi-coding-agent/src/cli/session_picker.rs`.

| Symbol / branch | Rust | Proof |
|---|---|---|
| `selectSession`, validation/cancel/selection | `select_session`, `SelectSessionOutcome` | existing `main_interactive` PTY picker cases |
| `loadSessions`, archive filtering and progress | `load_sessions` | `filters_by_archive_flag_and_forwards_progress` |
| Names come from each loaded `SessionInfo.name`, no duplicate name lookup | `is_named` | `metadata_names_order_named_sessions_first_then_by_modification_time` |
| Blank names are unnamed (JS trim) | `is_named` | `blank_names_do_not_count_as_named` |
| Active named-first ordering, then modified-desc within group | `load_sessions` | metadata-name and unnamed-order tests |
| Archived sessions ignore named-first ordering | `load_sessions` | `archived_sessions_are_ordered_only_by_modification_time` |
| Loader rejection propagates | `load_sessions` | `loader_errors_propagate` |

`select_session` passes the control DB path to the selector for mutations, not
for overlaying names. `SessionManager::list*` supplies authoritative metadata names.
Loader futures remain local to the TUI `LocalSet`; no additional dependency.

## Q18 native UTF-16 integration (2026-10-22)

Owned interactive APIs, selector state and frames now retain `JsString`; session search uses unit-based whitespace/token/phrase matching and shared `JsRegex`. Paths remain native through lexical operations and encode at filesystem/argv writes. Theme load errors use the original raw path. No terminal-display projection is used for lookup or stored text.

`lossless_interactive_selectors` covers raw picker selection/validation and unmatched-unit token/regex input. Tests are **unexecuted**: coding-agent library compilation is blocked. `/tmp/pi-rust-repin-lossless-interactive/interactive4.{json,log}` records compiler-only evidence (178 total errors, 77 in interactive_mode; other owned files zero primary errors), not behavioral parity or acceptance. Whole-mode migration and integration remain open; historical module “done” labels do not apply to this gate.
