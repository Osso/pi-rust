# coding-agent runtime-process traceability

Pinned source: `packages/coding-agent/src/core/runtime-process.ts` at `4f9e6f8a4c0065d81b10e7177b28d5082f0bc30d`. Earlier process/differential proofs below remain historical pre-repin evidence, not current-pin acceptance.

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

## Q18 native identity/error checkpoint

`ProcessIdentity.incarnation: Option<JsString>` preserves native units through shared JSON serde. `ProcessIdentityError.message` is JsString; Display alone encodes. Detached control parses incarnation directly, without `to_utf8().ok()` dropping a valid raw identity property.

`lossless_runtime_identity.rs` is a normal crate integration target. A minimal probe imports the actual entire runtime_process module by path (no copied code, DB scaffold or mocks). `runtime-identity-behavior-red.log` observes the old String field rejecting an unmatched unit during deserialize; `runtime-identity-green.log` reports **2/2 runner tests PASS, zero warnings**, with exact Node JSON roundtrip and original human error units retained before Display. Real process/liveness and all external callers still require full-library revalidation. Caller inventory lives in `/tmp/pi-rust-repin-lossless-tools/LANE.md` and `runtime-identity-outside-callers.txt`.

## Gaps

- macOS/BSD `ps -p <pid> -o command=` fallback for command lines is not ported; on non-Linux hosts `try_read_process_command_line` returns `None`, so `isPiRuntimeProcessAlive` reports `true` for any existing process and `isVerifiedPiRuntimeProcess` reports `false`. Process identity already requires Linux `/proc` in the source.

## Discrepancies

None.
