# auth-storage traceability

verified: 2026-10-21

Reference: frozen `4f9e6f8a4`, `packages/coding-agent/src/core/auth-storage.ts`, plus approved OAuth persistence delta `0c9f0390` in immutable checkout `26c8050f`. [Contract](../../specs/auth-storage.md); [delta lock](../delta-lock.json); [registry](oauth-registry.md).

## Integration status

Auth refresh continuation branch `repin/auth-refresh` restores the blocked patch and adds the approved delta and lossless boundary. This is not whole-module or provider-protocol parity. Final acceptance/clippy are parent-owned and not run here.

Patch provenance: `/tmp/pi-rust-repin-auth/blocked-public-refresh.patch`, SHA256 `787716f8000793de4adc221973300b99336f868c5a111dd7a963df655a6abb53`, baseline `51f6e772`. Hash and `git apply --check` succeeded before restoration. The disabled all-sync-fails branch was replaced, not retained. Prior partial integration and historical proofs: [restore](/tmp/pi-rust-repin-auth/RESTORE.md), [partial report](/tmp/pi-rust-repin-auth/REPORT.md), [partial verification](/tmp/pi-rust-repin-auth/VERIFY-PARTIAL.md).

Current commands, revisions, exact counts, mutations, and remaining failures: `/tmp/pi-rust-repin-auth-refresh/REPORT.md`. The requested workspace/all-targets compile check passes with zero warnings; touched differential targets are not all green.

## Current capability matrix

| Frozen/approved surface | Native mapping | Proof |
|---|---|---|
| Sync/async file lock, 30s stale, 15s heartbeat, callback compromise | FileAuthStorageBackend | auth_phase1/auth_phase1_process: actual reciprocal TS/Rust holders, cancellation/compromise before write, heartbeat retry, crashed owner recovery |
| Parent/file modes, read errors and UTF-8 decoding | FileAuthStorageBackend | auth_phase1_modes/auth_phase1_errors: nested 0700/file0600, exact EISDIR, Node replacement decoding |
| Public get/getAll/getProviderEnv/list/set/remove, raw credential values | JsObject/JsValue/JsString | auth_phase1_public + auth_phase1_lossless + auth_phase1_errors: reciprocal TS public writes, extras/env/unrelated entries, distinct surrogate/U+FFFD keys, exact touched-set file bytes |
| Runtime overrides, has/status, errors/reload, logout | SourceAuthState/public methods | auth_phase1_public |
| Constructors reload independently | from_storage/create; shared revision/cache removed | Auth public fixtures and all-targets compile; no obsolete shared-state parity claim |
| Typed OAuth access/refresh/extras | One OAuthCredential struct (OAuthCredentials alias), JsString tokens + JsObject extras | auth_phase1_lossless: getters, typed get/set, fake provider receives exact surrogate tokens/extras, touched rotation, reciprocal TS update |
| Successful expired OAuth rotation, rescue after callback/write failure | refresh_oauth_token_with_lock/get_api_key | auth_phase1_rotation: callback-return cancellation, locked write error, actual lock compromise, concurrent unrelated changes, fresh TS sees B, next refresh consumes B→C |
| Failed provider refresh / failed rescue with successful reload | get_api_key | auth_phase1_rotation: disk remains A, error surfaced, no fabricated rotation |
| All writes and reload fail | Approved delta: failed reload never certifies memory-only rotation | Both throw modes compare with zero delta differences; frozen baseline asserts exactly successful `new` versus failure/error/cause. Both retain memory B/disk A and exact errors |
| Key handoff to scalar-string request providers | Checked UTF-8 conversion only at handoff | Cached surrogate access remains in storage; handoff errors explicitly, never replaces |
| Builtin response/token handoff | Anthropic, Codex token response/account ID, Copilot access response parse losslessly | Anthropic surrogate token response RED/GREEN; ordinary provider test targets compile and pass; complete protocol proof remains separate |
| Login/getOAuthProviders | Registry interface | Existing fake registry/native pure mapping evidence; full login/callback/protocol parity remains pending |

## Lossless authority

AuthStorage has one authoritative JsObject snapshot. No parallel serde/raw envelope exists. Raw public getters, provider IDs and touched OAuth fields never cross a lossy serde boundary. Persistence uses parse_lossless/stringify_lossless; typed OAuth conversion holds JsString access/refresh and JsObject extras directly. Scalar-only legacy consumers use checked conversion and surface errors, not replacement.

`auth_phase1_lossless` proves distinct escaped surrogate provider keys and literal U+FFFD keys, nested surrogate keys/values, refresh-token rotation, typed touches, get/getAll/list, TS→Rust→TS updates and exact serialized output. The lossy touched-rotation mutant fails on token/key/value replacement.

## Approved discrepancy and oracle selection

The delta test selects auth-storage using delta-lock.json, checks selected HEAD/SHA256 immediately before spawning, and uses the mandatory sandbox's locked checkout validation. The common source lock is unchanged. Auth-storage imports its OAuth registry through the frozen package alias; the oracle registers into that same dependency, not an independently imported delta registry.

Frozen all-sync-fails behavior returns `new` with memory B/disk A. Approved behavior returns None, or throws the authentication wrapper whose cause is the persistence error. The test compares every other snapshot field exactly. Removing reload certification is a killed mutant.

## Remaining boundaries / retained scaffolding

- CredentialStore/read-only adapters remain because main/model runtime/resolver still consume them. They use canonical credential values; obsolete shared-revision/coalesced-read machinery is removed. The read-only adapter does not create auth files. Generic modify has no OAuth rotation rescue.
- In-memory backend still retains its old async serialization chain; this is not a frozen-concurrency parity claim.
- Full registry restore/order/error/model-filter and login/callback proofs remain pending. Copilot's old policy fallback still differs from frozen picker-only selection; provider protocol changes were excluded.
- Exact Node error shape/behavior for surrogate outbound HTTP/form/JSON fields is unproven. Stored/typed values are lossless; checked scalar handoffs fail explicitly. Nonbuiltin provider response parser revalidation remains separate.
- Touched differential targets expose missing retired models-store/model-runtime-test-utils imports and old extension/provider protocol differences. See report; no skips, source patches, fallback or weakened expected outputs added.
