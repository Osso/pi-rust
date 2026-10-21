# auth-storage traceability

Current source: `packages/coding-agent/src/core/auth-storage.ts` at frozen `4f9e6f8a4`.

## Integration status

Verified integration state: 2026-10-21. Canonical merge `4cb53c3143c0da5850242aa47629a001cfe69737` retains `51f6e772630fb455424ac721c1eae3a2f5647d57`, octal-fixture correction `4376b988`, and documentation correction `ba172341`. This is partial integration, not whole-module parity or a new waiver.

Retained-head evidence: 13 auth cases (one process-helper no-op; 12 substantive top-level cases) and one registry case passed. The later `4376b988` exact octal-only `auth_phase1` clippy run exited 0, fixing the earlier non-octal-permissions diagnostic; it is not a pass for the earlier multi-target auth clippy gate. Manual readability debt remains reported, not clean. Proof details and limitations: [partial verification](/tmp/pi-rust-repin-auth/VERIFY-PARTIAL.md).

The original auth worktree/branch was cleaned after merge. External patch SHA256 `787716f8000793de4adc221973300b99336f868c5a111dd7a963df655a6abb53` remains unapplied and not integration-ready; [restore provenance](/tmp/pi-rust-repin-auth/RESTORE.md) records its exact baseline and restrictions. Historical handoff wording in [REPORT.md](/tmp/pi-rust-repin-auth/REPORT.md) describes the pre-merge checkpoint, not a still-existing worktree.

## Phase1 revalidation

- `auth_phase1::sync_auth_contender_cannot_steal_three_second_live_async_lock`: actual healthy async holder; sync auth caller must use 30s stale, not global default.
- `auth_phase1::compromised_callback_cannot_write_under_lost_lock`: remove owned fixture lock during callback, allow 15s heartbeat to detect loss, then return write; write must be rejected.
- Async acquisition matches frozen retry count/min/max/factor; cancellation is a Rust-only boundary.
- `auth_phase1_modes::auth_creates_all_parent_directories_private`: nested parent creation must use 0700 for every newly created level, not chmod only the leaf. Auth file creation uses mode 0600 on open; chmod failures propagate instead of being suppressed.
- Public native source surface: `get`, `get_all`, `set`, `remove`, `has`, `has_auth`, `get_provider_env`, runtime overrides, status, `drain_errors`, `login`, `logout`, OAuth listing. `get_api_key`/refresh are segregated into an external blocked WIP patch pending sourcebug classification; absent from integration head. Frozen `list()` is temporarily named `list_providers()` to avoid colliding with obsolete trait `list(options)` callers.
- `auth_phase1_public`: TS public set → Rust read/set → TS reload and reverse OAuth update; accountId/email/sentinel extras, provider env, unrelated raw provider entries, modes, reload/error recovery, runtime precedence, ordering.
- `auth_phase1_process`: actual healthy TS process and Rust process, sync contenders at 3s and beyond 15s, observed mtime heartbeat, post-release acquisition; kill owned process group then age only its lock fixture to test stale recovery.
- External blocked WIP `auth_phase1_rotation`: fake provider returns B from consumed A, injected callback-return-before-write cancellation/write error/actual heartbeat compromise; rescue success, concurrent unrelated changes preserved, fresh TS process sees B, subsequent refresh consumes B→C, refresh error without rescue, failed rescue with successful subsequent reload. Ordinary cases proved before segregation; NOT implemented in offered partial head.
- `auth_source_discrepancy`: frozen TS successful API-key return despite persisted A when all sync operations fail after rotation; exact in-memory B/error ordering reproduced. Native refresh/get-api-key implementation removed to an external blocked patch pending user classification, not silently preserved/fixed; integration head has no production placeholder.
- Existing CredentialStore/read-only/shared-revision code remains obsolete compilation scaffolding, not current-pin parity evidence. No OAuth rescue was added to generic `modify`. Historical proof inventory below must not be read as revalidated.

## Remaining current-pin boundaries

- `auth_phase1_errors`: exact EISDIR public error, Node UTF-8 replacement decoding, untouched raw UTF-16 provider keys/values preserved byte-equivalently to source during unrelated public set via lossless merge.
- Raw UTF-16 preservation applies to persisted untouched entries, not the current `Value`-based public getter or typed OAuth credential adapter. Lone-surrogate public get/list and touched OAuth extra fields remain unsupported without a lossless public boundary; no waiver approved.
- Constructor still has obsolete shared-revision scaffolding; source per-instance reload semantics and native consumer removal remain pending integration. In-memory backend retains obsolete serialized-chain semantics pending whole-module completion.
- Source `getApiKey`, `refreshOAuthTokenWithLock`, error/reload/rescue branches absent from partial head; blocked WIP patch and proof ledger under `/tmp/pi-rust-repin-auth`.
- Login/provider callbacks, registry restore/order/error branches and built-in flows still need full differential proof; [registry trace](oauth-registry.md) separates tracked fake-provider evidence from external builtin mappings and records the old-flow bridge discrepancy. No whole-module done claim.

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

## Historical representation decisions (not current-pin proof)

- `auth.json` is held as `serde_json::Map` (insertion-ordered) so untouched providers keep their bytes
  and key order; only the modified provider is replaced.
- `serde_json::Map::remove` is a swap-remove (it moves the last entry into the removed slot), while V8
  `delete` preserves order. The port uses `shift_remove`; the cross-process delete test pins this.

## Historical gaps

- `AuthStorage` re-serializes a credential returned by `modify` through `serde_json`, so unknown extra
  fields on an api-key credential are dropped and OAuth extras follow the struct order
  (`type, refresh, access, expires`, then preserved extras). TypeScript preserves the modifier's object
  shape exactly. Recorded as a typed-boundary discrepancy.
- `with_lock_async` jitter uses wall-clock nanos instead of `Math.random()`; only affects retry spacing.

## Historical discrepancies

- None observed in file bytes for the ported operations (modify/delete/read/list).

## Historical proof inventory

- Focused cases: 16.
- Cross-implementation: 2 scenarios (byte parity + four read/write directions, both lock directions).

## Public refresh continuation

Restored `/tmp/pi-rust-repin-auth/blocked-public-refresh.patch`, SHA256 `787716f8000793de4adc221973300b99336f868c5a111dd7a963df655a6abb53`, original baseline `51f6e772`. Applicability checked against continuation master before restoring. Replaced its disabled branch with approved delta `0c9f0390`: failed reload cannot certify memory-only rotation. `auth_phase1_rotation::failed_rotation_persistence_and_reload_never_succeed_from_memory` reproduces all failures, both throw modes, disk A/memory B, exact recorded errors. Original RED: `/tmp/pi-rust-repin-auth-refresh/red-all-fail.log`. Lossless public boundary and delta differential proof continue separately.

Canonical continuation: public data is `JsObject`; `get/get_all/get_provider_env` retain `JsValue`; `list_providers` retains `JsString`. Shared-revision state removed, constructors reload. OAuthCredential (OAuthCredentials alias) holds JsString tokens and JsObject extras; locked rotation and rescue serialize those values directly. Legacy resolver adapters remain because main/model runtime still consume CredentialStore; they share canonical values, not a parallel serde snapshot. Native flow protocol proofs and external Unicode error wording remain gaps. Surrogate getter RED/GREEN logs are in `/tmp/pi-rust-repin-auth-refresh/`.

Approved delta differential: `auth_phase1_rotation::failed_rotation_persistence_and_reload_never_succeed_from_memory` selects auth-storage via `delta-lock.json`, verifies selected HEAD and SHA256 before spawn, and compares both throw modes with zero delta differences. Frozen baseline permits exactly result `new` versus failed native result (and error/cause fields in throw mode); memory B, disk A, backend failure boundaries and errors must otherwise match. Oracle uses the same pinned package OAuth registry as auth-storage imports (relative delta registry would be a second, incorrect registry).

`auth_phase1_lossless` covers raw get/getAll/list UTF-16 identity, distinct surrogate/U+FFFD provider keys, typed get/set, touched provider refresh with nested surrogate extras and tokens, reciprocal TS public update, exact bytes after touched set, and checked key handoff without altering stored tokens. `oauth_registry::builtin_response_handoff_preserves_utf16_tokens` reproduces and fixes Anthropic inbound token replacement. Codex token response/account ID and Copilot access response now use lossless parsing; their complete provider protocol suites are not part of this slice. Exact Node/HTTP Unicode failure wording remains unproven; no replacement is used.
