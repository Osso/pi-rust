# evals/scripts/run-evals.mjs

Pinned source: `packages/evals/scripts/run-evals.mjs` (97 lines), revision `532f7d12cd19235081bd4d6e425bde185652d86f`.
Rust: `crates/pi-evals/src/run_evals.rs`; entry point: `crates/pi-evals/src/bin/run-evals.rs`.
Behavior tests: `crates/pi-evals/tests/run_evals.rs`, executed against `CARGO_BIN_EXE_run-evals`.

## Coverage

| Source lines / branch | Rust | Behavioral proof |
| --- | --- | --- |
| 8–16: package root, nonempty artifact override, default ISO milliseconds + UUID | `PACKAGE_ROOT`, `read_artifact_directory`, `resolve_artifact_path` | `relative_artifacts_resolve_lexically_against_package_not_launch_directory`, `absolute_artifacts_do_not_canonicalize_symlinks_or_change_existing_permissions`, `default_artifacts_use_iso_timestamp_and_unique_uuid` (unset and empty override) |
| 23–39: separate flags, missing/empty/dash-prefixed value errors | `parse_arguments`, `Arguments::select` | `missing_separate_values_fail_before_side_effects`, `separate_and_equals_forms_trim_override_and_forward` |
| 40–50: equals forms, last occurrence wins, unknown arguments forwarded including `--` | `parse_arguments`, `Arguments::select` | `separate_and_equals_forms_trim_override_and_forward`, `equals_values_can_start_with_dash_or_contain_equals` |
| 52–66: ECMAScript trim, CLI precedence, CLI both required, environment both-or-neither | `trim_selection`, `resolve_selection` | `incomplete_cli_selection_never_falls_back_to_environment`, `incomplete_environment_selection_fails`, `environment_fallback_trims_both_values`, `absent_or_blank_selection_removes_child_environment` |
| 68–70, 85–88: resolve Vitest / invoke Node | `configure_child`, binary `main` | Approved native child-command adaptation below; `default_child_is_cargo_with_native_test_prefix` |
| 72–74: recursive mode 0700 mkdir before ordered stderr announcements | `create_artifact_directory`, `run` | `recursive_private_artifacts_and_ordered_stderr_precede_child_output`, `mkdir_failure_reports_error_without_announcing_or_spawning`; existing-directory permissions unchanged in `absolute_artifacts_do_not_canonicalize_symlinks_or_change_existing_permissions` |
| 75–84: inherit environment, absolute artifact override, selected model or deletion | `configure_child` | `separate_and_equals_forms_trim_override_and_forward`, `absent_or_blank_selection_removes_child_environment` |
| 85–97: package cwd, inherited stdio, spawn failure, exit status / signal | `configure_child`, `run`, binary `main` | `separate_and_equals_forms_trim_override_and_forward`, `child_inherits_stdin`, `recursive_private_artifacts_and_ordered_stderr_precede_child_output`, `spawn_failure_reports_error_and_nonzero_status`, `child_exit_codes_and_signal_are_propagated` (0, 3, SIGTERM → 1) |

## Differential proof

`pinned_node_script_matches_validation_selection_and_forwarding` copies the **unchanged pinned script** into a temporary package with a fake `node_modules/vitest/{package.json,vitest.mjs}`. Node executes the real script; the fake Vitest records selection, artifact directory, and forwarded arguments. Nineteen concrete argument/environment cases compare exit code, stdout, stderr, and normalized child arguments/selection against the actual Rust binary. Only the approved child-command prefix differs. Native tests separately assert cwd, inherited streams/environment, filesystem permissions, timestamp/UUID syntax, and failure behavior. No live providers or real evals run.

## Approved adaptation and proof boundaries

- Approved 2026-10-20: replace `node vitest.mjs run --config vitest.config.ts <args>` with `cargo test -p pi-evals <args>`; package root is `crates/pi-evals`. Vitest config loading is waived as JS-toolchain behavior.
- `PI_EVAL_CHILD_PROGRAM` (or module `run`'s executable parameter) substitutes the child program without changing its Cargo argument prefix. Default is `cargo`. Tests substitute an executable Python recorder, including a PATH-injected fake `cargo` for the default branch.
- Reuses existing `pi-ai::utils::uuid::random_uuid` and `pi-js-compat::{date::to_iso_string,string::js_trim}`; no dependencies added.
- Unix integration proof covers 0700 permissions and SIGTERM. Windows process/filesystem behavior is not exercised by this Unix test target; permission mode has no Windows equivalent, as in Node.
- Node throws on mkdir/spawn failure; Rust reports contextual operation/path errors and exits 1. Exact runtime stack-trace text is not a parity target; failure, stderr, and no-spawn boundaries are covered.
- No source bugs identified in this module.

## Reproduction

`cargo test -p pi-evals --test run_evals`

Development logs and revision-scoped proof ledger: `/tmp/pi-rust-run-evals/NN-*.log`, `/tmp/pi-rust-run-evals/ledger.md`.
