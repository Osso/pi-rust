# auth-storage traceability

Current source: `packages/coding-agent/src/core/auth-storage.ts` at frozen `4f9e6f8a4`.

## Phase1 revalidation

- `auth_phase1::sync_auth_contender_cannot_steal_three_second_live_async_lock`: actual healthy async holder; sync auth caller must use 30s stale, not global default.
- `auth_phase1::compromised_callback_cannot_write_under_lost_lock`: remove owned fixture lock during callback, allow 15s heartbeat to detect loss, then return write; write must be rejected.
- Async acquisition matches frozen retry count/min/max/factor; cancellation is a Rust-only boundary.
- `auth_phase1_modes::auth_creates_all_parent_directories_private`: nested parent creation must use 0700 for every newly created level, not chmod only the leaf. Auth file creation uses mode 0600 on open; chmod failures propagate instead of being suppressed.
- Public native source surface: `get`, `get_all`, `set`, `remove`, `has`, `has_auth`, `get_provider_env`, runtime overrides, status, `drain_errors`, `login`, `logout`, OAuth listing, `get_api_key`. Frozen `list()` is temporarily named `list_providers()` to avoid colliding with obsolete trait `list(options)` callers.
- `auth_phase1_public`: TS public set → Rust read/set → TS reload and reverse OAuth update; accountId/email/sentinel extras, provider env, unrelated raw provider entries, modes, reload/error recovery, runtime precedence, ordering.
- `auth_phase1_process`: actual healthy TS process and Rust process, sync contenders at 3s and beyond 15s, observed mtime heartbeat, post-release acquisition; kill owned process group then age only its lock fixture to test stale recovery.
- `auth_phase1_rotation`: fake provider returns B from consumed A, injected callback-return-before-write cancellation or write error; rescue success, unrelated preservation, refresh error without rescue, failed rescue with successful subsequent reload.
- `auth_source_discrepancy`: frozen TS successful API-key return despite persisted A when all sync operations fail after rotation; exact in-memory B/error ordering reproduced. Native branch explicitly disabled pending user classification, not silently preserved/fixed.
- Existing CredentialStore/read-only/shared-revision code remains obsolete compilation scaffolding, not current-pin parity evidence. No OAuth rescue was added to generic `modify`. Historical proof inventory below must not be read as revalidated.

## Historical pre-repin inventory (not current-pin proof)
Rust: `crates/pi-coding-agent/src/core/auth_storage.rs`.

Cross-implementation proof (`tests/auth_storage_cross_process.rs`, `oracle/auth-storage-oracle.ts`):

- The pinned TypeScript `AuthStorage` writes `auth.json`; the Rust port reads the same credential.
- Rust writes the same logical change into a fresh file; the bytes are identical.
- TypeScript reads what Rust wrote; deletes produce identical bytes in both runtimes.
- TypeScript holds `<auth.json>.lock`; the Rust read waits (measured ≥ 300 ms) and then succeeds.
- Rust holds the lock; TypeScript reports `ELOCKED`.

Focused tests: `tests/auth_storage.rs` (16 cases, ports of `test/auth-storage.test.ts`).

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `auth-storage.ts:44-53` | Backend contract: `withLock`/`withLockAsync` returning `{result, next}` | `AuthStorageBackend`, `LockOperation`, `AsyncLockOperation` (the caller captures results) |
| `auth-storage.ts:55-100` | `FileAuthStorageBackend`: create parent `0700`, create `auth.json` `0600`, lock with ten 20 ms attempts | `ensure_parent_dir`, `ensure_file_exists`, `acquire_sync_with_retry`; `auth_files_are_private` |
| `auth-storage.ts:102-175` | Async lock: 30 s stale, retries with exponential backoff and jitter, abort checks, compromise detection, release on abort | `acquire_async`, `jittered_delay`, `with_lock_async`; `aborts_while_waiting_*`, `pre_aborted_operations_*`, cross-process lock test |
| `auth-storage.ts:177-260` | `ReadOnlyAuthStorage`: strict validation, no resolution of command keys, no writes | `ReadOnlyAuthStorage`, `validate_credential`; `read_only_storage_validates_credentials` |
| `auth-storage.ts:262-292` | `InMemoryAuthStorageBackend` with a serialized async chain | `InMemoryAuthStorageBackend`; `in_memory_storage_implements_the_same_behavior`, `in_memory_backend_rejects_aborted_operations` |
| `auth-storage.ts:294-360` | Shared read state per path, constructor revision check | `SHARED_READ_STATE`, `from_storage_at`; `coalesces_file_reloads_*` |
| `auth-storage.ts:362-430` | Coalesced async reload with reader counting and cancellation | `Reload`, `start_reload`, `wait_for_reload`; coalescing and abort tests |
| `auth-storage.ts:432-470` | `read` resolves api-key config values (env/commands), OAuth unchanged | `resolved_credential`; command-key, scoped-env, OAuth tests |
| `auth-storage.ts:472-520` | `modify` merges into the freshly read file, preserves other providers, writes pretty JSON | `modify`; `modify_persists_*`, `serializes_concurrent_modifications`, cross-process bytes |
| `auth-storage.ts:522-536` | `delete` removes one provider in place | `delete` (uses `shift_remove` so remaining order matches V8); `delete_removes_one_*`, cross-process bytes |
| `auth-storage.ts:538-552` | `list` returns metadata in JavaScript key order | `list` via `js_json::object_entries`; delete/list test |
| `auth-storage.ts:554-570` | `readStoredCredential` one-off read | `read_stored_credential`; `stored_credentials_can_be_read_without_resolving` |

## Representation decisions

- `auth.json` is held as `serde_json::Map` (insertion-ordered) so untouched providers keep their bytes
  and key order; only the modified provider is replaced.
- `serde_json::Map::remove` is a swap-remove (it moves the last entry into the removed slot), while V8
  `delete` preserves order. The port uses `shift_remove`; the cross-process delete test pins this.

## Gaps

- `AuthStorage` re-serializes a credential returned by `modify` through `serde_json`, so unknown extra
  fields on an api-key credential are dropped and OAuth extras follow the struct order
  (`type, refresh, access, expires`, then preserved extras). TypeScript preserves the modifier's object
  shape exactly. Recorded as a typed-boundary discrepancy.
- `with_lock_async` jitter uses wall-clock nanos instead of `Math.random()`; only affects retry spacing.

## Discrepancies

- None observed in file bytes for the ported operations (modify/delete/read/list).

## Proof inventory

- Focused cases: 16.
- Cross-implementation: 2 scenarios (byte parity + four read/write directions, both lock directions).
