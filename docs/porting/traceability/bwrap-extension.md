# Bwrap extension (`extensions/bwrap/src`)

Port group: `crates/pi-coding-agent/src/bundled_extensions/bwrap/`.
Registered as `<first-party:bwrap>`; `backend.rs` is also consumed by the
bundled Pyrun extension for sandboxed runner invocations.

| Source | Rust | Tests |
|---|---|---|
| `backend.ts`: `resolveBwrapSandboxProfile` | `backend::resolve_bwrap_sandbox_profile` | differential `matches_typescript_bwrap_profiles_and_runner_environment` |
| `backend.ts`: `runBwrapAvailabilityCheck` | `backend::run_bwrap_availability_check` | tool-gate/status tests; real `bwrap --version` used by differentials |
| `backend.ts`: `buildBwrapInvocation`/arguments | `backend::build_bwrap_invocation` | differential `matches_typescript_bwrap_invocation_arguments` |
| `backend.ts`: `createSandboxedBashOperations` | `backend::create_sandboxed_bash_operations` | compiled through the bash tool override; real bwrap boundary exercised by `bwrap` availability |
| `backend.ts`: `createBwrapRunnerEnvironment` | `backend::create_bwrap_runner_environment` | differential `matches_typescript_bwrap_profiles_and_runner_environment` |
| `backend.ts`: `createBwrapRunnerCommand` + runtime/shebang read-only paths | `backend::create_bwrap_runner_command`, `runner_read_only_paths` | unit `parses_shebang_interpreter`, `derives_parent_directories`; invocation differential |
| `index.ts`: session-start status, tool gate, sandboxed read/write/edit/ls/find/bash/grep overrides, before-agent-start prompt, `/bwrap` command | `index::*` | registration/full-access behavior through `install_bwrap_extension`; sandbox tool bodies depend on a real pi executable as worker |
| `fs-worker.cjs`: all operations | `worker::run_operation_json` | differential `matches_node_fs_worker_operations_on_the_same_fixtures` runs the real Node worker on identical fixtures |
| `fs-worker.cjs` entry protocol | `worker::run_bwrap_fs_worker` via `internal_runner::run_internal_bwrap_fs_worker` | same differential plus write/mkdir round trip |

## Deviations / gaps

- Approved deviation: Node `fs-worker.cjs` is replaced by the pi executable's
  early argv mode `--internal-bwrap-fs-worker` (`internal_runner.rs`). Operations,
  payloads, stdout JSON, and exit codes match; argv differs only in the program
  path. Error stacks from Node are not reproduced; the worker writes the error
  message and exits 1.
- `user_bash` returns `{ operations }` in TypeScript, but the Rust
  `UserBashEventResult` stores JSON only and cannot carry a typed
  `BashOperations` handle. The handler is registered and returns an empty
  result; sandboxed user-bash execution is a gap pending a Rust-native
  operations field in the extension API.
- The full-access bash override creates the local bash definition with default
  options; detached-job/background-job wiring is owned by the bash tool port and
  is not re-applied here.
- `find`/`grep` glob matching uses `pi_js_compat::minimatch` for Node
  `path.matchesGlob`; regex matching uses the `regex` crate (no JS-only
  backreferences/lookaround). Recorded as a discrepancy.
- `fs-worker.cjs` reads files as UTF-8 for grep; the Rust worker uses
  `read_to_string` and skips the same expected read-error codes.
