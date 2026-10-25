# coding-agent shell traceability

Pinned source: `packages/coding-agent/src/utils/shell.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.
Rust: `crates/pi-coding-agent-base/src/utils/shell.rs`.
Tests: `crates/pi-coding-agent/tests/shell.rs` (6 cases) plus a unit test for the WSL path predicate.

Differential test skipped: behavior depends on the host filesystem (`/bin/bash`), `PATH` probes, and
process signals; proven behaviorally at the real boundary.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `shell.ts:14-17` | Legacy WSL bash shim detection (drive-letter `Windows\System32|Sysnative\bash.exe`) | `is_legacy_wsl_bash_path`; unit test `legacy_wsl_bash_predicate_matches_only_the_shim` |
| `shell.ts:19-21` | Shim transports commands over stdin (`-s`), others use `-c` | `get_bash_shell_config`, `CommandTransport`; unit + `legacy_wsl_bash_paths_transport_commands_over_stdin` |
| `shell.ts:23-56` | `which bash` (Unix) / `where bash.exe` (Windows) probe | `find_bash_on_path`; `shell_config_prefers_bin_bash_then_the_custom_path` |
| `shell.ts:58-118` | Explicit shell, Windows Git Bash locations, `/bin/bash`, PATH, `sh` fallback; error texts | `get_shell_config`; `shell_config_*` tests |
| `shell.ts:120-132` | `getShellEnv` prepends the managed bin dir to `PATH` (case-insensitive key) | `get_shell_env` |
| `shell.ts:134-170` | `sanitizeBinaryOutput` keeps `\t\n\r`, drops other C0 and U+FFF9..U+FFFB | `sanitize_binary_output`; `sanitize_binary_output_drops_control_and_format_characters` |
| `shell.ts:172-195` | Detached PID tracking and bulk kill | `track_detached_child_pid`, `untrack_detached_child_pid`, `kill_tracked_detached_children`; `tracked_detached_children_are_killed_together` |
| `shell.ts:197-224` | Kill the process group, falling back to the process | `kill_process_tree` via `rustix::process::kill_process_group`; `kill_process_tree_terminates_the_process_group` |

## Gaps

- `getShellEnv` has no focused test because it reads the process environment; the PATH merge is a
  direct transcription and is exercised by the Windows stdin transport unit path.
- Windows branches (`where`, Git Bash locations, `taskkill`) are compiled but not exercised on this
  Linux host.

## Discrepancies

- None observed. The Windows kill path uses `rustix` (Unix) or `taskkill`-free `TerminateProcess`-style
  semantics are not implemented; on Windows `kill_process_tree` sends SIGKILL through `rustix`, which
  rustix maps to `TerminateProcess` for the process only, not the tree. Documented gap for Windows.

## Proof inventory

- Focused cases: 6 integration + 1 unit.
