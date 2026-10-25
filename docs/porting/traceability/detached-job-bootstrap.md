# coding-agent detached-job-bootstrap traceability

Pinned source: `packages/coding-agent/src/core/detached-job-bootstrap.ts` at `4f9e6f8a4c0065d81b10e7177b28d5082f0bc30d`.

Proof: behavioral tests in `crates/pi-coding-agent/tests/detached_job_bootstrap.rs` against real processes, `/proc` descriptors, sessions, and files.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `PAYLOAD_GATE_SCRIPT` | Block on the gate until `run\n`, then `exec "$@"` | `PAYLOAD_GATE_SCRIPT`, prefixed with `exec 3<&0 </dev/null` to map stdin onto the source's fd 3 |
| `GatedDetachedPayloadInput` | args, command, cwd, env, identity/stderr/stdout paths | `GatedDetachedPayloadInput` |
| `spawnGatedDetachedPayload` spawn | `/bin/sh` gate, detached session, stdout+stderr on one file, gate pipe | `setsid(1)` + `/bin/sh` with piped stdin; `persists_identity_before_releasing_payload_with_direct_artifact_descriptors` asserts fds 1/2, parent pid, and session leadership |
| Missing pid | `Could not start detached payload: <command>` (Node: `/bin/sh` could not spawn, e.g. unusable `cwd`) | `DetachedPayloadError::SpawnFailed`; `reports_an_unusable_cwd_as_a_spawn_failure`. TS has no command pre-check: a missing command exits 127 inside the shell after release (`a_missing_command_fails_inside_the_shell_after_release`) |
| `env` / `command` lookup | `env: undefined` inherits `process.env`; `env` set replaces the child env exactly; `exec "$@"` resolves the command in the child's `cwd`/`PATH` | `env: None` → `process_env::vars()`, `Some` → manifest env only (`env_clear` + manifest); `resolves_a_relative_command_against_the_manifest_cwd`, `resolves_a_bare_command_against_the_manifest_path`, `replaces_the_child_environment_with_the_manifest_environment`, `inherits_the_process_environment_when_the_manifest_has_none` |
| `openSharedOutputFile` | Split paths rejected; `O_CREAT\|O_EXCL\|O_WRONLY\|O_APPEND`, mode `0600` | `open_shared_output_file`; `rejects_split_output_files`, `rejects_an_existing_output_file` |
| `persistIdentity` | `{pgid, pid, startTimeTicks}` written to `<path>.tmp`, fsynced, renamed, directory fsynced, mode `0600` | `persist_identity`/`write_identity_file`; main test asserts content, mode, and no leftover `.tmp` |
| `release` | Requires persisted identity; single use; writes `run\n` | `release`; `refuses_to_release_before_persisting_the_identity`, `refuses_a_second_release` |
| `waitForExit` | `{exitCode, signal}` | `wait_for_exit` via `spawn_blocking`; main test asserts `{0, None}` |
| Payload stdin | Ignored by the source (`stdio[0] = "ignore"`) | Payload reads `/dev/null`; `gives_the_payload_a_closed_stdin_after_release` |

## Approved deviation (user decision 2026-10-18)

Node uses `spawn(..., { detached: true })` plus a pipe on fd 3. `unsafe_code = "forbid"` rules out `pre_exec`, so the port composes the same shape from `setsid(1)` and a shell redirect: the parent's piped stdin is copied to fd 3, stdin is redirected to `/dev/null`, and `exec "$@"` keeps the pid. Consequences:

- Linux-only (the module already depends on `/proc` for identity).
- The payload's stdin is `/dev/null` instead of `/dev/null`-equivalent inherited ignore — identical observable behavior.
- `setsid(1)` is located on this process's `PATH` (`find_setsid`), never on the payload's manifest `PATH`; if absent the spawn fails with `Could not start detached payload`.

## Discrepancies

None beyond the approved mechanism deviation.

## Current re-pin audit

Whole module read against frozen 4f9e6f8a4. Open errors previously leaked Rust OS text; actual-source EEXIST regression `/tmp/runtime-services-red-bootstrap-error.log` now maps the output/identity file open boundary through the shared Node filesystem message authority. GREEN pending at commit. Existing real gate/session/descriptors/env/cwd/output/identity tests retained. Exotic descriptor/fsync/entropy faults and unsupported OS runtime shapes are excluded by the current parity bar.
