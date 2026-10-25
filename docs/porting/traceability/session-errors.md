# Session import error

Common pin `4f9e6f8a4`, `coding-agent/src/core/session-errors.ts`, all 12 lines. Native: `crates/pi-coding-agent-storage/src/core/session_errors.rs`. [Runtime contract](../../specs/agent-session-runtime.md).

`SessionImportFileNotFoundError` retains the original UTF-16 `filePath`, formats `File not found: <path>` and sets its source name. `into_callback` uses the existing native Error projection with `filePath` and retains the existing Rust error as `CallbackError.original`; the interactive catch can distinguish it from an ordinary Error without a text-prefix heuristic. Base Error membership and stack omission follow the shared carrier policy.

`agent_session_runtime::missing_import_preserves_the_frozen_error_name_and_file_path` invokes the actual pinned runtime import method and compares the entire serialized error value. `ui_tests::import_error_class_controls_recovery_and_exit_like_frozen` compares full real-mode error frames and actual process exit codes for a missing import versus a later factory Error with similar text. Both are RED→GREEN. Source: `oracle/runtime-import-error-oracle.mts`, `oracle/interactive-startup-oracle.mts`; artifacts and boundaries are recorded in [runtime traceability](agent-session-runtime.md#missing-import-error-identity-2026-10-25).
