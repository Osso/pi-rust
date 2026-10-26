# coding-agent runtime-process traceability

Pinned source: `packages/coding-agent/src/core/runtime-process.ts` at `4f9e6f8a4c0065d81b10e7177b28d5082f0bc30d`. Whole-module Linux audit completed 2026-10-25. Current-pin targeted execution passed at ccb73a6d2; the ledger below names the proofs.

Differential proof: request `27012` compares `commandLineIsPiRuntime` across 13 command lines (basenames, entrypoint suffixes, backslash paths, near-miss extensions, empty argv). Behavioral proof: `crates/pi-coding-agent/tests/runtime_process.rs` runs against real `/proc` entries, a real zombie, and real child processes.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `ProcessIdentity` | `{pid, startTimeTicks, incarnation?}` | `ProcessIdentity` |
| `PI_RUNTIME_ENTRYPOINT_SUFFIXES` | Three entrypoint paths | `PI_RUNTIME_ENTRYPOINT_SUFFIXES` |
| `readProcessStat` | Linux `/proc/<pid>/stat`: state, start time (field 22), rejects missing `)`, non-safe-integer or zero start time | `read_process_stat`; errors `Invalid /proc stat for process {pid}` / `Invalid process stat for process {pid}`; zombie test |
| `readProcessIdentity` | Identity only | `read_process_identity`; live/zombie tests |
| `isProcessIdentityAlive` | Same start time and state not `Z`/`X` | `is_process_identity_alive`; `matches_a_live_process_only_at_its_exact_start_time`, zombie test |
| `readProcessGroupId` | `/proc` process-group field | `read_process_group_id`; same-process frozen snapshot differential GREEN. |
| `isSupersededRuntimeIdentity` | Same pid/start and both incarnations present but different (including empty string) | `is_superseded_runtime_identity`; twelve-pair frozen differential. |
| `isPiRuntimeProcessAlive` | Own pid → true; missing → false; unreadable cmdline → true; else runtime check | `is_pi_runtime_process_alive`; `identifies_pi_runtime_processes_by_command_line` |
| `isVerifiedPiRuntimeProcess` | Unreadable cmdline → false | `is_verified_pi_runtime_process`; same test |
| `processExists` | `kill(pid, 0)`; `EPERM` counts as existing | `test_kill_process` + `Errno::PERM`; `reports_missing_processes_as_dead` |
| `tryReadProcessCommandLine` | Linux `/proc/<pid>/cmdline`, NUL-split, empty parts dropped | `try_read_process_command_line` |
| `commandLineIsPiRuntime` | `pi`/`pi.exe` basename or a normalized argument equal to/ending with an entrypoint suffix | `command_line_is_pi_runtime`; focused cases and differential `27012` |

## Q18 native identity/error checkpoint

`ProcessIdentity.incarnation: Option<JsString>` preserves native units through shared JSON serde. `ProcessIdentityError.message` is JsString; Display alone encodes. Detached control parses incarnation directly, without `to_utf8().ok()` dropping a valid raw identity property.

`lossless_runtime_identity.rs` is a normal crate integration target. A minimal probe imports the actual entire runtime_process module by path (no copied code, DB scaffold or mocks). `runtime-identity-behavior-red.log` observes the old String field rejecting an unmatched unit during deserialize; `runtime-identity-green.log` reports **2/2 runner tests PASS, zero warnings**, with exact Node JSON roundtrip and original human error units retained before Display. Real process/liveness and all external callers still require full-library revalidation. Caller inventory lives in `/tmp/pi-rust-repin-lossless-tools/LANE.md` and `runtime-identity-outside-callers.txt`.

## Gaps

- Darwin native binding/identity and non-Linux `ps` command-line branches are not ported (assigned Darwin gap); Rust identity requires Linux `/proc`.
- Adversarial stat contents, numeric extremes and OS/runtime fault shapes are outside the parity bar.

## Re-pin proof ledger

- `suite_core_2` filter `runtime_identity`: missing-process error RED at `/tmp/runtime-services-red-identity.log` (Rust OS text vs frozen `ENOENT: no such file or directory, open '/proc/4194303/stat'`); new superseded export RED at `/tmp/runtime-services-red-exports.log`. GREEN at ccb73a6d2; `/tmp/runtime-services-green-leaves.log`.
- Existing `runtime_process` real-process tests cover live, absent, zombie, own and foreign processes. Existing `runtime_process_matches_typescript` covers command lines; all five current-pin behavior tests GREEN at ccb73a6d2 (`/tmp/runtime-services-green-process.log`).

## Discrepancies

None.

The changed module tests all pass. The 16-test core-leaf batch also exposed an unrelated project-resolver fixture failure (`key must be a string` at differential_core_leaves.rs:641); its proof is not claimed here. Empty-executable RED: `/tmp/runtime-services-red-empty-executable.log`.

## Retained UTF-8 boundary (2026-10-26)

`read_process_identity` retains strict `/proc/.../stat` reading. Frozen `runtime-process.ts:38` uses UTF-8 replacement, but this is kernel data rather than a user-authored text file; invalid-byte process names remain an OS/path edge gap. See the [complete file-read audit](../utf8-decoding-sweep.md).
