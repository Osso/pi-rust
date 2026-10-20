# evals/scripts/run-evals.mjs

Pinned source: `packages/evals/scripts/run-evals.mjs` (97 lines), revision `532f7d12cd19235081bd4d6e425bde185652d86f`.
Rust: `crates/pi-evals/src/run_evals.rs`; entry point: `crates/pi-evals/src/bin/run-evals.rs`.
Behavior tests: `crates/pi-evals/tests/run_evals.rs`, executed against `CARGO_BIN_EXE_run-evals`.

## Coverage

| Source lines / branch | Rust | Behavioral proof |
| --- | --- | --- |
| 1–20: imports, package root, nonempty artifact override, default ISO milliseconds + UUID, argument state | `PACKAGE_ROOT`, `read_artifact_directory`, `resolve_artifact_path`, `Arguments` | `relative_artifacts_resolve_lexically_against_package_not_launch_directory`, `absolute_artifacts_do_not_canonicalize_symlinks_or_change_existing_permissions`, `default_artifacts_use_iso_timestamp_and_uuid_v4_syntax` (unset and empty override); signed epoch milliseconds use shared `to_iso_string`, without a pre-epoch error path |
| 22–35: separate flags, missing/empty/dash-prefixed value errors | `parse_arguments`, `Arguments::select` | `missing_separate_values_fail_before_side_effects`, `separate_and_equals_forms_trim_override_and_forward` |
| 36–47: equals forms, last occurrence wins, unknown arguments forwarded including `--` | `parse_arguments`, `Arguments::select` | `separate_and_equals_forms_trim_override_and_forward`, `equals_values_can_start_with_dash_or_contain_equals` |
| 49–63: ECMAScript trim, CLI precedence, CLI both required, environment both-or-neither | `trim_selection`, `resolve_selection` | `incomplete_cli_selection_never_falls_back_to_environment`, `incomplete_environment_selection_fails`, `environment_fallback_trims_both_values`, `absent_or_blank_selection_removes_child_environment` |
| 65–67, 83–85: resolve Vitest / invoke Node | `configure_child`, binary `main` | Approved native child-command adaptation below; `default_child_is_cargo_with_native_test_prefix` |
| 69–71: recursive mode 0700 mkdir before ordered stderr announcements | `create_artifact_directory`, `run` | `recursive_private_artifacts_and_ordered_stderr_precede_child_output`, `mkdir_failure_reports_error_without_announcing_or_spawning`, `pinned_node_mkdir_diagnostics_match_before_announcements` (ENOTDIR, EACCES); existing-directory permissions unchanged in `absolute_artifacts_do_not_canonicalize_symlinks_or_change_existing_permissions` |
| 72–82: inherit environment, absolute artifact override, selected model or deletion | `configure_child` | `separate_and_equals_forms_trim_override_and_forward`, `absent_or_blank_selection_removes_child_environment` |
| 83–91: package cwd, inherited stdio, synchronous child execution | `configure_child`, `run` | `separate_and_equals_forms_trim_override_and_forward`, `child_inherits_stdin`, `recursive_private_artifacts_and_ordered_stderr_precede_child_output` |
| 93–95: throw spawn error | `run`, shared `NodeFsError::console_message`, binary `main` | `spawn_failure_reports_error_and_nonzero_status`, `node_spawnsync_enoent_diagnostic_matches_with_forwarded_arguments` |
| 97: child exit status, null/signal → 1 | `run`, binary `main` | `child_exit_codes_and_signal_are_propagated` (0, 3, SIGTERM → 1) |

## Differential proof

`pinned_node_script_matches_validation_selection_and_forwarding` copies the **unchanged pinned script** into a temporary package with a fake `node_modules/vitest/{package.json,vitest.mjs}`. Node executes the real script; fake Vitest records selection, artifact directory, and forwarded arguments. Nineteen concrete argument/environment cases compare exit code, stdout, stderr, and normalized child arguments/selection against the actual Rust binary. Only the approved child-command prefix differs.

`pinned_node_mkdir_diagnostics_match_before_announcements` uses the same unchanged-script fixture for ENOTDIR and EACCES. Native stderr is compared unchanged against Node stderr with only JS stack frames/source-excerpt headers and the Node version footer removed: the `Error: <message>` line and complete own-property block remain. Error name, message, errno, code, syscall, path, property order, and exit 1 match. Neither launcher announces or spawns after mkdir failure.

The pinned script spawns `process.execPath`, so making its executable missing is not a faithful practical fixture. `node_spawnsync_enoent_diagnostic_matches_with_forwarded_arguments` instead executes `node -e`, calls `spawnSync('cargo', nativeArgs, {env: ...})` with a PATH containing no Cargo, and throws `result.error`. Three cases cover the native prefix, quoting/control characters, and compact multirow `spawnargs`. The Rust binary has the same failure boundary and compares exact diagnostics/status, retaining prior announcements. Shared inspect rendering has additional Node comparison in `pi-js-compat/tests/node_fs_errors.rs::console_messages_match_node_inspection`.

Native tests separately assert cwd, inherited streams/environment, filesystem permissions, timestamp/UUID syntax, and failure behavior. The Cargo runner mounts the checkout read-only; the default-directory test uses a nested Bubblewrap mount to overlay only the child's package root with a writable temporary fixture, preserving the enclosing sandbox. No live providers or real evals run.

## Approved adaptation and proof boundaries

- Approved 2026-10-20: replace `node vitest.mjs run --config vitest.config.ts <args>` with `cargo test -p pi-evals <args>`; package root is `crates/pi-evals`. Vitest config loading is waived as JS-toolchain behavior.
- Binary and module always run `cargo`. Tests put an executable Python recorder named `cargo` first on PATH; no executable-selection environment seam remains. The default-child test proves `PI_EVAL_CHILD_PROGRAM` cannot override Cargo.
- Reuses `pi-ai::utils::uuid::random_uuid` and `pi-js-compat::{date::to_iso_string,string::js_trim,node_fs::node_fs_error_record}`. Shared `NodeFsError::console_message` supplies Node inspect-style properties, string quoting and spawnargs layout; no dependencies added.
- Unix integration proof covers 0700 permissions, EACCES and SIGTERM. Windows process/filesystem behavior is not exercised by this Unix test target; permission mode has no Windows equivalent, as in Node.
- Only JS-specific error source excerpts/stack frames are omitted from error presentation. Node's version footer is runtime identity, not an error property. Error names/messages/order/status are not waived.
- No source bugs identified in this module.

## Reproduction

`cargo test -p pi-evals --test run_evals`

Shared formatter proof: `cargo test -p pi-js-compat --test node_fs_errors`.

Development logs and revision-scoped proof ledger: `/tmp/pi-rust-run-evals/NN-*.log`, `/tmp/pi-rust-run-evals/ledger.md`.
