# internal-runner.ts traceability

Source: `packages/coding-agent/src/internal-runner.ts` (19 lines, pinned revision in
`docs/porting/source-lock.json`). Rust: `crates/pi-coding-agent/src/internal_runner.rs`.

The bash runner body (`core/detached-bash-runner.ts`) is ported in
`core::detached_bash_runner`; the pyrun runner body is owned by the pyrun slice and is not ported.
This module ports the dispatch, manifest validation, and error reporting around them.

## Exported symbols

| TS symbol | Rust | Tests |
| --- | --- | --- |
| `runInternalDetachedRunner` | `run_internal_detached_runner` | `cli_entry.rs` (6 tests) |
| — (`DETACHED_BASH_RUNNER_MODE`) | `DETACHED_BASH_RUNNER_MODE` | `each_mode_dispatches_to_its_runner` |
| — (`DETACHED_PYRUN_RUNNER_MODE`) | `DETACHED_PYRUN_RUNNER_MODE` | same |

The mode constants are imported from the runner modules in TypeScript; the bash constant is
re-exported from `core::detached_bash_runner` and the pyrun constant is declared here. Values
match the source byte for byte.

## Branches

| # | Branch | Test |
| --- | --- | --- |
| 1 | `args[0]` is neither mode → `false` (not handled) | `other_argv_is_not_an_internal_runner` |
| 2 | missing `args[1]` → throw `Internal detached runner ${mode} requires a launch manifest path` | `missing_manifest_throws_the_exact_message` |
| 3 | bash mode → `runDetachedBashRunner(manifestPath)` | `each_mode_dispatches_to_its_runner` |
| 4 | pyrun mode → `runDetachedPyrunRunner(manifestPath)` | same |
| 5 | runner success → handled, no exit code | `success_does_not_create_an_error_file` |
| 6 | runner failure → write `<manifest>.runner-error` with `message + "\n"` and mode `0o600`, `exitCode = 1`, still handled | `runner_failure_writes_a_private_runner_error_file` |

## Deviations

- **Error message** — TypeScript writes `error.stack ?? error.message`; Rust hosts return a
  message only, so the stack line is absent.
- **`process.exitCode`** — returned as `InternalRunnerOutcome::exit_code` for the binary
  bootstrap to apply.
- **`writeFileSync` mode** — `OpenOptions::mode(0o600)` on Unix matches the source's creation
  mode; an existing file keeps its own mode in both implementations.

## Proof

`cargo test -p pi-coding-agent --test cli_entry` (9 tests, including the 6 runner cases).
