# Auth storage

Frozen `packages/coding-agent/src/core/auth-storage.ts` at `4f9e6f8a4c0065d81b10e7177b28d5082f0bc30d`; all563 lines audited. Approved persistence delta `0c9f0390` is separately locked at checkout26c8050f; common source bytes are unchanged. [Contract](../../specs/auth-storage.md), [delta lock](../delta-lock.json).

| Frozen exports / branches | Native mapping and proof |
| --- | --- |
| FileAuthStorageBackend constructor/path normalization, parent0700, file0600, UTF-8 read/write/chmod | FileAuthStorageBackend; public/backend modes/errors fixtures, actual TS/Rust reciprocal bytes and lock holders |
| Sync lock:10 attempts,20ms ELOCKED retry,30s stale, release in finally | Source/native contention and best-effort snapshot fixtures; source lock behavior retained |
| Async lock:10 randomized exponential100ms–10s retries,30s stale/15s heartbeat; compromise before callback/after callback/after write; ignore unlock failures | auth_phase1/process real release barriers, heartbeat/compromise/crashed-holder fixtures; rotation rescue tests cover callback/write boundaries |
| InMemoryAuthStorageBackend: independent callback snapshots, optional next write, raw callback rejection | Backend no-next/raw-rejection source comparisons and overlapping expired refresh source/native proof |
| AuthStorage create/fromStorage/inMemory/reload: independent cached snapshots; errors retain old snapshot; no implicit getter reload | Actual-source snapshot-reloads/raw-get matrix plus load-error/recovery tests |
| parseStorageData / recordError / drainErrors | Native lossless JSON, native/shared Error carrier, one-shot drain. New malformed-file RED proves SyntaxError brand was replaced with Error; constructor/message now preserved through the shared native constructor |
| persistProviderChange/set/remove/logout: reload after load error, merge current file, exact pretty bytes/no newline, preserve unrelated providers/order, record and propagate failure | Public reciprocal TS→Rust→TS/native reverse fixtures; concurrent public sets, removal failure, errors/reload recovery, ordered raw updates |
| get/getProviderEnv/list/has/getAll: raw records, own keys, copied provider env, JS key order | Source raw-get/no-command-execution, numeric-key ordering, native UTF-16/distinct surrogate/U+FFFD keys and extras |
| Runtime override set/remove, hasAuth, getAuthStatus | Stored/runtime/environment precedence and labels; logout removes only storage, runtime override stays until explicitly removed |
| login / getOAuthProviders | Actual frozen command→AuthStorage→registry callback/persistence differential: complete provider ids/names/order, success/failure, exact auth.json bytes; no live providers |
| getApiKey: truthy runtime, stored config key/env, known/unknown OAuth, fresh/expired, includeFallback, environment fallback | Public source/config/key-value fixtures; exact TypeError constructor/message for invalid persisted key values; lossless public returned tokens |
| Locked OAuth refresh: reload snapshot, type/fresh guards, requested credential refresh, exact type-first spread/write, successful rotation rescue | Actual frozen/Codex wrapper file bytes; lossless rotation, overlapping in-memory refreshes, real compromised/write/rescue boundaries |
| Refresh failure: record/reload, use another persisted rotation only after successful reload, optional authentication wrapper/cause | Both throw modes against approved delta; all-sync-fails frozen baseline discrepancy asserted separately; no memory-only success certification |

## Ordered credential correction

Shared OAuthCredential keeps key-order metadata beside canonical typed values/extras, not another raw credential copy. Parsed known/extra positions survive typed updates. Codex constructs access/refresh/expires/accountId/optional email in frozen order; AuthStorage performs the literal type-first spread for login, locked refresh and rescue. Direct nonempty id-token email wins; empty direct email falls through to profile.email; absent/undecodable tokens omit email. Frozen atob byte-string decoding remains unchanged.

REDs `/tmp/settings-models-codex-order-red.log` and `codex-email-red.log` prove reordered file bytes and missing email. GREEN `codex-order-green.log`:7 actual frozen AuthStorage→builtin Codex refresh cases plus native order/email tests. The old lossless fixture hardcoded refresh-first although its callback mutated access-first input in place; it now runs the matching actual frozen in-place callback and compares full credential JSON and complete file bytes. No fields/order are normalized or stripped.

## Old-only API removal

Parent approved deleting CredentialStore read/modify/delete, resolving reads, read_stored_credential and its root export. Both lane and merged parent146eb5622 were searched before deletion. Remaining all-target consumers discovered by parent were migrated at5f55fa7f0: raw-error tests use get/set/getApiKey; runtime tests use public AuthStorage overrides/remove/logout. Source logout does not clear overrides. The old no-create one-off reader/overlay-delete contracts are removed, not emulated. Earlier ReadOnlyAuthStorage/from_storage_at aliases remain removed.

## Recorded boundaries

- Gap: inherited Object.prototype credential names use own-key lookup only, under the approved native-object policy.
- Gap: pathological/invalid record shapes, numeric extremes, OS/runtime fault shapes and scheduling oddities remain outside the user-observable parity bar; no carrier/API is added for them.
- Owned getters follow the common approval. Public values/tokens remain JsString/JsValue through persistence; only actual Node HTTP/form/header/stdout boundaries encode or reject them. Provider protocol implementation is owned by its separate source rows, not this storage module.

## Current proof

-17 public/backend/reciprocal tests: `/tmp/settings-models-auth-adapter-green.log`.
-8 heartbeat/compromise/public/error fixtures and private parent modes: `auth-backend-public-green.log`, `auth-modes-green.log`.
-4 lossless/overlap plus3 rotation/rescue cases: `auth-carrier-regressions-green.log`.
-18 actual-source login scenarios plus native callback/stdio/real-pty fixtures: `login-green.log`.
-Codex/Copilot20 and Anthropic6 native regressions: `oauth-regressions.log`, `anthropic-regressions.log`.
-Explicit parent-requested `cargo check -j3 --workspace --all-targets` PASS at5f55fa7f0, zero warnings: `workspace-all-targets-5f55fa7f0.log`.

All short log names above are under `/tmp/settings-models-`. Current malformed-JSON constructor GREEN and final changed-line clippy follow this commit in `/tmp/settings-models-proof-ledger.md`. Historical reports under `/tmp/pi-rust-repin-auth-refresh/` are prior-slice evidence, not current blockers or whole-workspace acceptance.

Current closure proof: 062551724 malformed JSON3-source-case constructor/message GREEN, `/tmp/settings-models-auth-json-error-green.log`; current full backend/public8 plus modes1 GREEN, `auth-backend-public-green.log`/`auth-modes-green.log`. Source/native login18 scenarios and9 active native cases GREEN at a8c36bbd2, `login-green.log`. Warning-mode clippy passes in `auth-json-clippy.log`/`login-clippy.log`, no changed-line warnings. Existing long oracle dispatch remains legacy test setup, not production logic; new callback helpers were split and changed lines reviewed for readability.
