# coding-agent detached-job-retention traceability

Pinned source: `packages/coding-agent/src/core/detached-job-retention.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Differential proof: request `27014` runs 11 candidate/policy fixtures through the pinned module and the Rust port: age-boundary selection, over-cap selection, path tie-breaks (lowercase and mixed case), protected-only overflow, empty input, within-cap input, and three invalid-input rejections. Focused tests: `crates/pi-coding-agent/tests/detached_job_retention.rs`.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `DetachedArtifactRetentionCandidate` | `directoryPath`, `byteSize`, `terminalAt`, `protectedByLiveReference` | `DetachedArtifactRetentionCandidate` (f64 sizes/timestamps, like JS numbers) |
| `DetachedArtifactRetentionPolicy` | `now`, `maxAge`, `maxBytes` | `DetachedArtifactRetentionPolicy` |
| `requireNonnegativeFinite` | `<name> must be a nonnegative finite number` | `require_nonnegative_finite`; `rejects_invalid_nonnegative_finite_values`, `rejects_a_non_finite_terminal_timestamp` |
| `validateRetentionInput` | Policy first, then candidates in input order (`byteSize`, then `terminalAt`) | `validate_retention_input`; focused tests assert the first failing field |
| `compareCandidates` | `terminalAt` ascending, then `directoryPath.localeCompare` | `compare_candidates` + `compare_paths` via ICU4X collation in the host locale; `uses_the_directory_path_as_a_deterministic_tie_break_for_equal_timestamps`, differential mixed-case cases |
| `findExpiredCandidates` | Unprotected and `terminalAt <= now - maxAge` | `find_expired_candidates`; boundary case |
| `selectExpiredAndOverCapCandidates` | Expired paths first, then oldest unprotected non-expired paths until retained bytes fit | `select_expired_and_over_cap_candidates`; over-cap and protected-overflow cases |
| `selectDetachedArtifactDirectoriesToDelete` | Validates, sorts, selects; returns paths | `select_detached_artifact_directories_to_delete` returning `Result`; differential `27014` |

## Exclusions

- Numeric fields are `f64` to preserve JavaScript validation semantics (`NaN`/`Infinity` rejection); callers pass integer byte sizes and timestamps.
- The error is a `Result<String, String>` instead of a thrown `Error`; the message text is identical.

## Discrepancies

- `localeCompare` uses ICU collation resolved from `LC_ALL`/`LC_COLLATE`/`LANG` (falling back to `und`), matching Node's ICU-backed default-locale comparison. Both processes in differential `27014` run with the same environment; a host with a different locale would order ties by that locale in both implementations.
