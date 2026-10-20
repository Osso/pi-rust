# session-picker traceability

Source: `packages/coding-agent/src/cli/session-picker.ts` at authorized `c248b0067`
([reference lock](../session-name-source-lock.json), approved 2026-10-20).
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
