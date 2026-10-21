# coding-agent runtime-process traceability

Pinned source: `packages/coding-agent/src/core/runtime-process.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Differential proof: request `27012` compares `commandLineIsPiRuntime` across 13 command lines (basenames, entrypoint suffixes, backslash paths, near-miss extensions, empty argv). Behavioral proof: `crates/pi-coding-agent/tests/runtime_process.rs` runs against real `/proc` entries, a real zombie, and real child processes.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `ProcessIdentity` | `{pid, startTimeTicks, incarnation?}` | `ProcessIdentity` |
| `PI_RUNTIME_ENTRYPOINT_SUFFIXES` | Three entrypoint paths | `PI_RUNTIME_ENTRYPOINT_SUFFIXES` |
| `readProcessStat` | Linux `/proc/<pid>/stat`: state, start time (field 22), rejects missing `)`, non-safe-integer or zero start time | `read_process_stat`; errors `Invalid /proc stat for process {pid}` / `Invalid process stat for process {pid}`; zombie test |
| `readProcessIdentity` | Identity only | `read_process_identity`; live/zombie tests |
| `isProcessIdentityAlive` | Same start time and state not `Z`/`X` | `is_process_identity_alive`; `matches_a_live_process_only_at_its_exact_start_time`, zombie test |
| `isPiRuntimeProcessAlive` | Own pid → true; missing → false; unreadable cmdline → true; else runtime check | `is_pi_runtime_process_alive`; `identifies_pi_runtime_processes_by_command_line` |
| `isVerifiedPiRuntimeProcess` | Unreadable cmdline → false | `is_verified_pi_runtime_process`; same test |
| `processExists` | `kill(pid, 0)`; `EPERM` counts as existing | `test_kill_process` + `Errno::PERM`; `reports_missing_processes_as_dead` |
| `tryReadProcessCommandLine` | Linux `/proc/<pid>/cmdline`, NUL-split, empty parts dropped | `try_read_process_command_line` |
| `commandLineIsPiRuntime` | `pi`/`pi.exe` basename or a normalized argument equal to/ending with an entrypoint suffix | `command_line_is_pi_runtime`; focused cases and differential `27012` |

## Gaps

- macOS/BSD `ps -p <pid> -o command=` fallback for command lines is not ported; on non-Linux hosts `try_read_process_command_line` returns `None`, so `isPiRuntimeProcessAlive` reports `true` for any existing process and `isVerifiedPiRuntimeProcess` reports `false`. Process identity already requires Linux `/proc` in the source.

## Discrepancies

None.

Q16: `q16_new_exports::superseded_identity_matches_frozen_matrix` covers same/different PID/start marker, same/different/missing incarnation. Darwin `readDarwinProcessInfo` remains unimplemented: frozen module loads architecture-specific `process-info.node`, validates safe creation/group markers and R/Z/X states. Linux host cannot execute this native contract; no new deviation approval inferred.
