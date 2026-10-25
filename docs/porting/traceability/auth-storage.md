# auth-storage traceability

## Q15c held-lock proof correction

Actual frozen comparison (`q15c-auth-held-lock-source-proof.log`) proves a held lock exhausts ordinary synchronous read retries and yields the best-effort absent snapshot; release restores the credential. The old cross-process test incorrectly expected an asynchronous wait from read. It now compares real frozen withLockAsync and the native async backend directly under explicit release-file barriers, retaining credential equality, minimum wait and reverse ELOCKED checks. `q15c-auth-lock-and-runtime-green.log` GREEN6 includes three cross-process cases plus runtime overlay tests. No production lock behavior changed.

## Q15c contract migration (2026-10-24)

Historical adapter modifier-rejection proof (`q15c-auth-store-modifier-rejection-{red,green}.log`) is retired with that old-only API. Replacement cases exercise actual frozen/native InMemoryAuthStorageBackend callback rejection, preserving the raw value and unchanged storage.

Frozen auth-storage backend withLockAsync and GetApiKeyOptions have no operation signal; removed old cancellation parameter/check/select paths. The old-only CredentialStore adapter is removed in the settings/models lane. Callback rejection carriers remain native. [Current type contract](../../specs/auth-types.md). Historical operation-cancellation proof below is retired, not common-pin parity.

verified: 2026-10-23

Reference: frozen `4f9e6f8a4`, `packages/coding-agent/src/core/auth-storage.ts`, plus approved OAuth persistence delta `0c9f0390` in immutable checkout `26c8050f`. [Contract](../../specs/auth-storage.md); [delta lock](../delta-lock.json); [registry](oauth-registry.md).

## Snapshot and raw lookup correction (2026-10-23)

Both queued `auth_storage` failures were stale expectations, not frozen AuthStorage regressions. Frozen auth-storage.ts:218 constructs an independent instance and reloads;265–279 reload synchronously and explicitly;317 returns the cached raw value. No read/list reload coalescing or ReadOnlyAuthStorage class exists in that source.

Actual frozen `auth-storage-oracle.ts` probes now back `file_stores_keep_independent_snapshots_until_explicit_reload`: two stores keep old snapshots after an external edit; reloading one updates only that instance; backend read effects remain2/2/3/4 at the four observations. `frozen_get_reads_raw_credentials_without_resolution_execution_or_validation` compares complete raw get/getAll/provider/error results for an env reference, command reference, numeric key and empty-array file; auth bytes stay unchanged and a command sentinel is never created.

Old-only ReadOnlyAuthStorage and the unused from_storage_at compatibility alias were deleted. Its bespoke modify-rejection assertion has no frozen API equivalent and was removed with the class; surviving raw-read/no-command-execution behavior is reproven through frozen get. Focused target:16/16 GREEN. This does not establish malformed getApiKey input parity or whole auth/provider protocol acceptance.

## Persisted API-key config values

`differential_auth_key_value::persisted_api_key_values_keep_node_resolution_and_type_errors` invokes frozen `AuthStorage.create/getApiKey` against real auth.json files: missing/null/boolean/number/array/object/non-callable startsWith keys, empty/raw-surrogate strings and an env reference. Compares returned UTF-16 or exact error name/message/constructor/TypeError identity. RED: missing key returned None natively instead of throwing. `parse_api_key_config` now preserves the frozen parser failure through the existing StorageError/CallbackError carrier. At `5eaf1d3f`, all10 persisted-value cases and the9-case warning-frame matrix pass. Raw getters are unchanged.

## Integration status

Auth refresh continuation branch `repin/auth-refresh` restores the blocked patch and adds the approved delta and lossless boundary. Verifier fixes preserve public UTF-16 key returns, Node header error codes, and overlapping in-memory refreshes. Requested focused sandbox tests, workspace/all-targets check and strict selected-target clippy pass; no independent verification delegated. This is not whole-module or provider-protocol parity.

Patch provenance: `/tmp/pi-rust-repin-auth/blocked-public-refresh.patch`, SHA256 `787716f8000793de4adc221973300b99336f868c5a111dd7a963df655a6abb53`, baseline `51f6e772`. Hash and `git apply --check` succeeded before restoration. The disabled all-sync-fails branch was replaced, not retained. Prior partial integration and historical proofs: [restore](/tmp/pi-rust-repin-auth/RESTORE.md), [partial report](/tmp/pi-rust-repin-auth/REPORT.md), [partial verification](/tmp/pi-rust-repin-auth/VERIFY-PARTIAL.md).

Current commands, revisions, exact counts, mutations, and remaining failures: `/tmp/pi-rust-repin-auth-refresh/REPORT.md`. The requested workspace/all-targets compile check passes with zero warnings; all selected auth_phase1/registry/provider-boundary tests pass (Anthropic required a bounded serial retry after a callback-port collision). Historical touched differential targets are not all green and were not rerun for these fixes.

## Approved own-key gap (2026-10-23)

| Frozen branch | Native behavior | Classification |
| --- | --- | --- |
| auth-storage.ts:361–373 `hasAuth`/`getAuthStatus` read inherited ordinary-object names as stored credentials | Own credential keys only; prototype-only names are absent | Approved gap under the no-inherited-Object.prototype policy in AGENTS.md. No carrier or prototype emulation is added. Runtime overrides and own stored records retain their separate exact behavior. |

## Current capability matrix

| Frozen/approved surface | Native mapping | Proof |
|---|---|---|
| Sync/async file lock, 30s stale, 15s heartbeat, callback compromise | FileAuthStorageBackend | auth_phase1/auth_phase1_process: actual reciprocal TS/Rust holders, cancellation/compromise before write, heartbeat retry, crashed owner recovery |
| Parent/file modes, read errors and UTF-8 decoding | FileAuthStorageBackend | auth_phase1_modes/auth_phase1_errors: nested 0700/file0600, exact EISDIR, Node replacement decoding |
| Public get/getAll/getProviderEnv/list/set/remove, raw credential values | JsObject/JsValue/JsString | auth_phase1_public + auth_phase1_lossless + auth_phase1_errors: reciprocal TS public writes, extras/env/unrelated entries, distinct surrogate/U+FFFD keys, exact touched-set file bytes |
| Runtime overrides, has/status, errors/reload, logout | SourceAuthState/public methods | auth_phase1_public |
| Constructors reload independently; getters hold per-instance snapshots until explicit reload | from_storage/create/get/get_all/reload | Actual-frozen `auth_storage::file_stores_keep_independent_snapshots_until_explicit_reload`; external-file edits and backend read effects compared, no obsolete coalescing claim |
| Typed OAuth access/refresh/extras | One OAuthCredential struct (OAuthCredentials alias), JsString tokens + JsObject extras | auth_phase1_lossless: getters, typed get/set, fake provider receives exact surrogate tokens/extras, touched rotation, reciprocal TS update |
| Successful expired OAuth rotation, rescue after callback/write failure | refresh_oauth_token_with_lock/get_api_key | auth_phase1_rotation: callback-return cancellation, locked write error, actual lock compromise, concurrent unrelated changes, fresh TS sees B, next refresh consumes B→C |
| Failed provider refresh / failed rescue with successful reload | get_api_key | auth_phase1_rotation: disk remains A, error surfaced, no fabricated rotation |
| All writes and reload fail | Approved delta: failed reload never certifies memory-only rotation | Both throw modes compare with zero delta differences; frozen baseline asserts exactly successful `new` versus failure/error/cause. Both retain memory B/disk A and exact errors |
| Public get_api_key UTF-16 return | Result<Option<JsString>>; no encoding/validation | auth_phase1_lossless compares frozen getApiKey U+D800 result and units [55296], preserving stored credential |
| Builtin response/token handoff | Anthropic, Codex token response/account ID, Copilot access response parse losslessly | Anthropic surrogate token response RED/GREEN; ordinary provider test targets compile and pass; complete protocol proof remains separate |
| Login/getOAuthProviders | Registry interface | Existing fake registry/native pure mapping evidence; full login/callback/protocol parity remains pending |

## Lossless authority

AuthStorage has one authoritative JsObject snapshot. No parallel serde/raw envelope exists. Raw public getters, provider IDs and touched OAuth fields never cross a lossy serde boundary. Persistence uses parse_lossless/stringify_lossless; typed OAuth conversion holds JsString access/refresh and JsObject extras directly. Scalar-only legacy consumers use checked conversion and surface errors, not replacement.

`auth_phase1_lossless` proves distinct escaped surrogate provider keys and literal U+FFFD keys, nested surrogate keys/values, refresh-token rotation, typed touches, get/getAll/list, TS→Rust→TS updates and exact serialized output. The lossy touched-rotation mutant fails on token/key/value replacement.

## Approved discrepancy and oracle selection

The delta test selects auth-storage using delta-lock.json, checks selected HEAD/SHA256 immediately before spawning, and uses the mandatory sandbox's locked checkout validation. The common source lock is unchanged. Auth-storage imports its OAuth registry through the frozen package alias; the oracle registers into that same dependency, not an independently imported delta registry.

Frozen all-sync-fails behavior returns `new` with memory B/disk A. Approved behavior returns None, or throws the authentication wrapper whose cause is the persistence error. The test compares every other snapshot field exactly. Removing reload certification is a killed mutant.

## Remaining boundaries / retained scaffolding

- Old-only CredentialStore read/modify/delete, resolving reads, one-off read_stored_credential and its root export are removed. The public get/set/remove/reload and backend lock APIs are the only storage authorities; earlier ReadOnlyAuthStorage/from_storage_at aliases also remain removed.
- In-memory async callbacks take independent snapshots without an async chain. auth_phase1_lossless compares overlapping public expired refreshes against frozen: both consume A and return new, persisting B. File backend locking is unchanged.
- Full registry restore/order/error/model-filter and login/callback proofs remain pending. Copilot's old policy fallback still differs from frozen picker-only selection; provider protocol changes were excluded.
- Node differential proves outbound JSON escapes, form USVString replacement, and header TypeError name/message/code (ERR_INVALID_CHAR) without mutating tokens. CallbackError.code preserves boundary error identity; stored/typed values remain lossless. Nonbuiltin provider response parser revalidation remains separate.
- Touched differential targets expose missing retired models-store/model-runtime-test-utils imports and old extension/provider protocol differences. See report; no skips, source patches, fallback or weakened expected outputs added.

## Q18 native credential/config flow

Config literal/template values, native env values, command cache and resolver return text use `JsString`. Names are scalar only after ASCII env-name grammar validation; unmatched units are no-match at that single parser site. Command text encodes at spawn/stdin only. Credential storage uses shared `JsValue` serde without scalar projection. Added config/raw credential tests in `raw_error_consumer_components`, **unexecuted** while coding-agent is compiler-blocked. Model registry call adaptation pending.

## Old-only adapter retirement (2026-10-25)

Parent approved deleting the old-only CredentialStore impl and read_stored_credential export. Both this lane and merged parent146eb5622 were searched: only native tests/root re-export consumed them; no production consumer was found. Tests now use actual frozen public get/getApiKey/set/remove, real concurrent synchronous sets, explicit reload snapshots and direct async backend lock callbacks. Reciprocal file-byte, release-barrier/minimum-wait/ELOCKED, private-mode, no-next and raw-rejection assertions are retained at those real boundaries. Creating a public file-backed AuthStorage creates missing auth.json as frozen does; get itself does not add credentials. The old no-create one-off helper contract is removed, not emulated. No production provider/refresh algorithm changed. Focused GREEN follows the deletion commit.

## Provider credential order / Codex email (2026-10-25)

Parent assigned the shared OAuthCredential carrier/order correction and the complete Codex credentialsFromToken idToken/email branch to this lane. RED `/tmp/settings-models-codex-order-red.log` compares actual frozen AuthStorage.getApiKey through the real builtin Codex refresh against native AuthStorage plus the real native Codex refresh with only HTTP/clock injected: file bytes differed (refresh/access instead of access/refresh). A native round-trip also lost interleaved extra positions. Separate RED `/tmp/settings-models-codex-email-red.log` proves dropped email.

The canonical credential now carries only key-order metadata beside its existing typed values/extras, not another raw credential copy. to_js_value preserves captured order and appends newly added properties; Codex constructs access/refresh/expires/accountId/email in frozen order. AuthStorage uses an explicit type-first spread boundary for login, locked refresh and rotation rescue. Direct nonempty email wins; empty direct email falls through to profile.email; undecodable/absent id tokens omit email. Frozen atob byte-string decoding is retained. Actual-source wrapper cases compare full credential JSON and exact complete auth.json bytes, including unrelated records. Focused GREEN follows the commit.
