# lockfile traceability (support module)

Rust: `crates/pi-coding-agent/src/utils/lockfile.rs`.
Source contract: `proper-lockfile` 4.1.2 (`lib/lockfile.js`, `lib/mtime-precision.js`, `lib/adapter.js`)
as used by `core/auth-storage.ts`, `core/trust-manager.ts`, and `core/models-store.ts`.

The TypeScript and Rust runtimes must share one agent directory, so the on-disk protocol is the
contract: a directory named `<file>.lock` (or `options.lockfilePath`), its mtime as the liveness
signal, and `stale`/`retries` rules.

## Common-pin auth dependency revalidation

Frozen `4f9e6f8a4` proper-lockfile `lib/lockfile.js:updateLock`: threshold uses the acquired options.stale, not a separate primitive default. Transient stat/utimes failures retry after 1s until stale; missing lock or changed mtime compromises immediately. Error code is ECOMPROMISED while retaining the underlying filesystem error.

`auth_phase1::auth_heartbeat_permission_error_uses_auth_thirty_second_threshold`: make only the owned parent inaccessible, wait through first 15s heartbeat, restore access before 30s, observe renewal within 1s retry. Behavioral RED exposed the hardcoded 10s threshold. Global default and other callers' stale policy unchanged.

`auth_phase1_process` proves actual healthy TS/Rust holders across first heartbeat, sync exclusion at 3s/16s, release and owned crash/stale recovery.

## Historical pre-repin cross-implementation proof

`crates/pi-coding-agent/tests/lockfile_cross_process.rs` runs the pinned `proper-lockfile` through
`oracle/lockfile-cross-process.ts` and proves both directions:

- TypeScript holds and refreshes a lock; Rust `acquire` with retries waits for the release (measured
  wait ≥ 400 ms) and then succeeds; Rust `check` sees the TypeScript lock.
- Rust holds a lock; TypeScript `lock` reports `ELOCKED`; after the Rust release TypeScript acquires.
- A live, refreshed TypeScript lock is not reclaimed even with a 1 s stale window.

Focused tests: `crates/pi-coding-agent/tests/lockfile.rs` (7 cases).

| `proper-lockfile` contract | Rust mapping and proof |
|---|---|
| `acquireLock` uses `mkdir(lockfilePath)` as the atomic operation | `fs::create_dir`; `acquire_creates_a_lock_directory_and_release_removes_it` |
| `EEXIST` + not stale → `ELOCKED` `"Lock file is already being held"` with `error.file` | `locked_error`; `a_held_lock_reports_elocked_and_keeps_the_directory` |
| `stat` mtime `< Date.now() - stale` → `rmdir` then re-acquire | `is_stale` + `acquire_until_locked`; `a_stale_lock_directory_is_removed_and_reacquired` |
| `stat` `ENOENT` → retry once with `stale: 0` | `recreate`; exercised by the cross-process release race |
| `stale = max(stale \|\| 0, 2000)`; `update = clamp(update ?? stale/2, 1000, stale/2)` | `normalized_stale_ms`, `normalized_update_ms` |
| `probe` sets mtime to `ceil(now/1s)*1000 + 5` and records `s`/`ms` precision | `probe_mtime`, `ceil_to_second`; mtime-ahead assertion |
| Refresh: compare mtime with ours, `utimes`, compromise on mismatch/`ENOENT`/threshold | `spawn_refresh`, `refresh_once`; `async_acquisition_keeps_the_mtime_fresh_and_reports_compromise` |
| `getMtime` rounds up to the next second for `s` precision | `refreshed_mtime` |
| Release deletes the directory; second release is `ERELEASED` `"Lock is already released"` | `release_state`; compromise test asserts the released code |
| `toSyncOptions` rejects retries with `ESYNC` `"Cannot use retries with the sync api"` | `acquire_sync`; `sync_acquisition_rejects_retries` |
| `retry` backoff: factor 2, 1000 ms minimum, no randomization | `retry_delay`; `retries_wait_for_the_holder_to_release` |
| `check` reports whether a non-stale lock exists | `check`; `custom_lockfile_path_and_check_report_lock_state` |
| `realpath: false` skips canonicalization; default resolves through `realpath` | `lockfile_path` |

## Gaps

- `onExit` cleanup of held locks at process exit is not ported; `Drop` removes the directory, and a
  hard kill leaves a lock that goes stale after `stale` ms, exactly as a killed TypeScript process does.
- `unlockSync` has no caller in the ported modules and is not exposed.
- Compromise error text for a vanished directory reproduces Node's `ENOENT: no such file or directory,
  stat '<path>'` message because `auth-storage` rethrows that message to callers.

## Discrepancies

- None observed: mtime, stale, retry, and error-code behavior match the pinned implementation.

## Proof inventory

- Focused cases: 7.
- Cross-implementation cases: 2 scenarios, 3 protocol directions.
