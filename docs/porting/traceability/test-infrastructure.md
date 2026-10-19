# Test infrastructure traceability (ca-errno)

No TypeScript module is ported by this slice. It fixes three cross-cutting test/production defects that
made `pi-coding-agent` tests report misleading errno codes, fail intermittently under parallel load, and
leave Node/tsx processes running after a test process died.

## 1. Node errno mapping

Covered in [child-process.md](child-process.md#shared-node-errno-mapping-ca-errno). One
`utils::child_process::{node_errno_name, node_error_code}` implementation now serves every spawn message
and filesystem code; `tests/errno_codes.rs` proves `ENOENT`, `EACCES`, `ETXTBSY`, and `ENOTDIR` at the
real `execve` boundary for the pyrun runner and bwrap availability check.

## 2. Test-script spawn race (`ETXTBSY`)

### Root cause

`tests/*` created executable scripts with `fs::write` and then `exec`'d them. While thread A held the
script open for writing, a concurrent `fork` in thread B (any `Command::spawn`) inherited the write
descriptor until its `exec`. If A's `execve` ran in that window, Linux returned `ETXTBSY`. The old
hand-rolled errno maps collapsed that code to `EIO`, so the intermittent failure looked like a generic
spawn error.

Reproduction with the old writer (4 writer threads, 4 fork-storm threads, 400 scripts each): the
`execve` failed with `Os { code: 26, kind: ExecutableFileBusy }` in 3/3 runs.

### Fix

`crates/pi-coding-agent/tests/support/executable_script.rs::write_executable_script` writes the script
through a short-lived `sh` child (`cat > "$1" && chmod 755 "$1"`). The test process never opens the
script for writing, so no sibling fork can inherit the descriptor. Production code has no retry; the
race is removed from the test writer only.

Every test that writes-then-execs a script uses it: `pyrun_extension`, `bundled_extensions_context`,
`bash_stdin`, `config_install`, `differential_config`, `desktop_notification`,
`support/clipboard_harness`, `differential_package_manager`, `differential_session_tools`, `grep_rg`,
`ls_rtk`, `package_manager`, `package_manager_git`, `tools_manager`.

### Proof

- `tests/executable_script_race.rs`: 4 writer threads + 4 fork-storm threads, no failures (5/5 runs).
- Old-writer control (deleted after the experiment): 3/3 runs failed with `ETXTBSY`.
- 20 consecutive runs of `pyrun_extension` + `bundled_extensions_{commands,context,gates,run_plan,tools}`
  + `executable_script_race` with default test parallelism: 20/20 passed, 0 failures.

## 3. Orphaned oracle processes

### Root cause

`TsOracle` (`crates/*/tests/support/oracle.rs`, 10 copies) and `ModeHarness` spawned `tsx`. `tsx` runs
the oracle in a **child** Node process. `Drop` killed only the `tsx` wrapper, and a killed test binary
never runs `Drop` at all. In both cases the Node child was reparented to systemd and kept running;
observed instances included hours-old `bundled-extensions-oracle.ts` processes and loop-extension
timers that keep the event loop alive after stdin EOF.

### Fix

- Oracle and mode-harness children are spawned with `CommandExt::process_group(0)`, so the `tsx` wrapper
  is a process-group leader and its Node/esbuild descendants share that group.
- `oracle/parent-watchdog.mjs` is preloaded into every oracle/mode harness via `NODE_OPTIONS`
  (`--import`). It exits when `PI_ORACLE_PARENT_PID` (the test process, set by the Rust support code)
  disappears or when its own parent changes, and it sends `SIGKILL` to its process group first. Because
  the wrapper is the group leader, either process noticing the dead test process takes the whole tree
  down.
- `Drop` kills the process group with `rustix::process::kill_process_group` (safe, no `unsafe_code`
  exception) and then reaps the wrapper. `ModeHarness` now has a `Drop` at all (it previously had none)
  and takes its `Child` out of an `Option` so a completed `finish()` is not killed again.
- `tests/oracle_lifecycle.rs` re-executes its own binary as a holder that spawns a real oracle and
  publishes the wrapper + descendant PIDs. Two tests cover the shutdown paths:
  - `oracle_process_tree_exits_when_the_test_process_is_killed` (`SIGKILL` of the holder; no `Drop`).
  - `oracle_process_tree_exits_when_the_holder_drops_the_oracle` (normal holder exit; `Drop` runs).
  Both assert every recorded PID is gone within 10 s.

### Proof

- `tests/oracle_lifecycle.rs`: 40/40 runs passed (3 tests each).
- Sensitivity control: removing the `NODE_OPTIONS` watchdog from `support/oracle.rs` makes the
  `SIGKILL` case fail with the wrapper (`ppid=2476`) and Node child still alive after 10 s; restoring it
  passes. The fixture `oracle/lifecycle-oracle.ts` holds a timer and never reads stdin, so stdin EOF
  alone cannot mask the watchdog.
- `ps` after the stress runs shows no `lifecycle-oracle`, `core-leaves-oracle`, or `oracle_lifecycle`
  processes.

## 4. Pre-existing test-isolation flake found while proving #2

`bundled_extensions_context.rs` mutated the process-global `pi_js_compat::process_env` overlay
(`PI_CLAUDE_MEMORY`, `XDG_CACHE_HOME`) in two tests that ran concurrently; one test's `clear()` could
make the other spawn the real `claude-memory` CLI, producing a huge, environment-dependent enrichment
string against the oracle's `Null`. A `static Mutex` (`ENV_LOCK`) now serializes the two env-mutating
tests. Not caused by the errno/watchdog changes, but it blocked the required 20× parallel proof.

## Files

- Production: `crates/pi-coding-agent/src/utils/child_process.rs`, `core/tools/node_fs.rs`, and the
  spawn/fs call sites listed in [child-process.md](child-process.md).
- Test support: `crates/pi-coding-agent/tests/support/executable_script.rs`,
  `crates/pi-coding-agent/tests/support/oracle.rs`, `crates/pi-coding-agent/tests/support/mode_harness.rs`,
  and the nine other `tests/support/oracle.rs` copies.
- Oracle: `oracle/parent-watchdog.mjs`, `oracle/lifecycle-oracle.ts`.
- Tests: `tests/errno_codes.rs`, `tests/executable_script_race.rs`, `tests/oracle_lifecycle.rs`.
- Dependencies: `rustix = 1.1.5` (`process`) as a development dependency of the crates whose tests own
  an oracle; see `docs/dependencies.md`.
