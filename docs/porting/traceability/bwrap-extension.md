# Bwrap extension (`extensions/bwrap/src`)

Port group: `crates/pi-coding-agent-extensions/src/bundled_extensions/bwrap/`.
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
| `index.ts`: `user_bash` returns `{ operations }` | `index::register_user_bash_handler` returns `create_sandboxed_bash_operations`; `ExtensionRunner::emit_user_bash` carries it and `rpc_mode::run_bash` passes it to `AgentSession::execute_bash` | `user_bash_operations_reach_execute_bash` |
| `fs-worker.cjs`: all operations | `worker::run_operation_json` | differential `matches_node_fs_worker_operations_on_the_same_fixtures` runs the real Node worker on identical fixtures, including lookahead, lookbehind, and backreference patterns |
| `fs-worker.cjs` entry protocol | `worker::run_bwrap_fs_worker` via `internal_runner::run_internal_bwrap_fs_worker` | same differential plus write/mkdir round trip |

## Q18 backend native checkpoint

Common pin: `4f9e6f8a4`. Invocation/runner command, cwd, environment keys/values, extra mounts, shell paths, shebang and interpreter/runtime discovery paths use `JsString`. Path resolution/canonicalization errors propagate through native results rather than copied-path fallbacks. Encoding occurs only at FS, argv, env and spawn boundaries; returned canonical paths are decoded after the FS boundary. No sandbox is launched by the new raw-unit builder fixture.

`backend::tests::invocation_keeps_raw_cwd_command_and_environment_until_spawn` asserts raw cwd/command/env in the actual invocation object using a real temporary directory. Existing helper tests use native contracts. Tests remain unexecuted pending coding-library compilation. `index.rs` general worker/SandboxParams/runtime command/cwd fields and worker argv/env construction are now native too. Current-executable/CWD failures propagate instead of choosing `pi`/empty-CWD fallbacks; worker stdout/stderr read/join errors no longer masquerade as empty successful output. Sandbox Operations reuse the shared native IO error carrier. Status and before-agent-start text retain native cwd/command units.

`operation_tests::worker_invocation_keeps_raw_workspace_and_payload_units_until_argv` covers the real invocation DTO and serialized worker workspace/path, without spawning a sandbox. It is unexecuted until the coding library compiles. Historical differential rows above are not new acceptance proof.

Re-pin correction: the full-access operation now passes its configured shell into `create_local_bash_operations`; a real fake-shell process test failed before this correction and passes after it.

## Deviations / gaps

- Approved deviation: Node `fs-worker.cjs` is replaced by the pi executable's
  early argv mode `--internal-bwrap-fs-worker` (`internal_runner.rs`). Operations,
  payloads, stdout JSON, and exit codes match; argv differs only in the program
  path. Error stacks from Node are not reproduced; the worker writes the error
  message and exits 1.
- Full-access bash forwards the context lifecycle and detach registry; its lifecycle controller owns the same multi-agent store, so no parallel store field is introduced in BashBackgroundJobs.
- `find`/`grep` matching now uses raw UTF-16 minimatch AST and the shared native
  ECMAScript regex adapter. Actual frozen worker/Node options, exact tested
  failure branches, engine provenance, and explicit approved limits are in
  [worker matching](worker-matching.md); no scalar-only matcher is retained.
- `fs-worker.cjs` reads files as UTF-8 for grep; the Rust worker uses
  `read_to_string` and skips the same expected read-error codes.
- Session-start status: the source reads `ctx.ui.theme`, which throws when no
  theme is initialized; the runner records the handler error and skips the
  status. The Rust handler returns the same error from `try_current_theme()`, so
  the runner records it identically and `pi extensions`/`tools` do not abort
  before `initTheme`.

## Whole-module re-pin audit (2026-10-25)

Frozen pin: `4f9e6f8a4c0065d81b10e7177b28d5082f0bc30d`. Backend exports and every helper were compared: profile selection, availability, environment, base/system/symlink/extra/workspace mounts, runner lookup and shebang/venv paths, parent dedup, bash output/timeout/abort settlement. Empty PYTHONPATH/TERM values are omitted only in their truthy source branches; empty LANG/PATH/USER stay explicit. Custom full-access shell forwarding is restored.

Index audit covers worker invocation/parse/abort, all operation adapters, MIME lookup, grep payload/truncation/notices, registration, session start/gate, full-access/sandbox tool dispatch, user bash, prompt replacement/append and status command. `/bwrap` checks availability and reports exact error-level blocked text; signalled worker status prints `null`, not `-1`; zero match-limit does not print a notice. Tests use invocation-only builders and fake executables, never real sandboxes.

| Status | Source branch | Evidence / remaining item |
|---|---|---|
| done | Backend environment/invocation and profiles | Two frozen differentials including empty env; `full_access_preserves_configured_shell` real process. |
| done | Worker operations/error propagation, MIME and lossless argv | Eight `operation_tests`; signalled fake worker and zero-limit regressions RED→GREEN. |
| done | `/bwrap` unavailable and full-access notifications | `status_command_reports_unavailable_sandbox_but_not_full_access` RED→GREEN. |
| done | Index full-access `backgroundJobs`/`detachRegistry` forwarding | `bwrap-detached-lifecycle` RED→GREEN proves the installed extension detaches via registry, observes Completed through the lifecycle and retains exact ready/done output from a real child runner. |
| gap | non-Linux unsupported-platform statuses/prompts | Linux is the implemented runtime; Windows/non-Linux execution is not exercised. |
| gap | adversarial JSON, exotic fds, OS faults and scheduling races | Approved user-observable parity exclusions. |

Proof log/revision mapping: [lane ledger](../bundled-ext-b-proof.md). Both backend and index rows close. The temporary detached-job gap is fixed, not accepted as a deviation.
