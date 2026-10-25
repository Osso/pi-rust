# Auth contracts

Frozen `packages/ai/src/auth/types.ts:8–182` defines request credentials and auth callbacks. Native declarations live in `crates/pi-ai/src/auth/types.rs`; [traceability](../porting/traceability/auth-types.md) maps representations and proof.

## What it must do

### API surface

- [x] Expose credential read, serialized modify and delete only; no list or operation cancellation options. Modify receives current credentials; absent replacement leaves them unchanged.
- [x] Require the actual chat/image model for API-key resolution; no status-check or resolution-signal interface.
- [x] Represent optional API-key login by callback presence. OAuth has name/login/refresh/toAuth only; no subscription/login-label/refresh-signal compatibility fields.

### Values and failures

- [ ] Retain OAuth credential property order across typed updates, including interleaved known/extra keys; provider construction order survives AuthStorage persistence.

- [x] Preserve native UTF-16 auth names, credentials, event text and callback values; expose only auth_url/device_code/progress event shapes and the four frozen prompt kinds.
- [x] Carry context env/file-existence rejections, notify throws and credential modifier failures through the shared native error carrier, without stringifying or silently discarding the reason.
- [x] Preserve actual abort(undefined) defaulting to branded AbortError/code20, distinct from null and other falsy values.
- [x] Preserve the classified Codex onAuth-before-try listener leak; caller rejection and outer manual-input abort still occur.

## How it works

- [Auth type/source mapping](../porting/traceability/auth-types.md)
- [Shared reason carrier](abort-signals.md)
- [App storage and lock boundaries](../porting/traceability/auth-storage.md)

## Implementation inventory

| Path | Role |
|---|---|
| crates/pi-ai/src/auth/types.rs | Native records, traits, optional login callback and boxed AuthError. |
| crates/pi-ai/src/auth/{helpers,context,credential_store,resolve}.rs | Contract implementations and request/error propagation. |
| crates/pi-ai/src/auth/oauth/*.rs; src/providers/{amazon_bedrock,cloudflare_auth,google_vertex}.rs | Login notification and context failure consumers. Other flow-module parity is separate. |
| crates/pi-ai/src/oauth_registry.rs | Native extension callback adapter. |
| `crates/pi-coding-agent-models/src/core/auth_storage.rs`, `crates/pi-coding-agent-data/src/core/runtime_credentials.rs` | Public app storage and runtime credential implementation. |
| crates/pi-coding-agent-interactive/src/modes/interactive/{interactive_mode,login_options,components/oauth_selector}.rs | UI notifier and local login-method discriminator. |

## Tests asserting this spec

| Path | Contract |
|---|---|
| oracle/auth-types-contract.mts | Actual frozen export/type surface. |
| crates/pi-ai/tests/auth_types_contract.rs; oracle/auth-types-errors-oracle.ts | Required model/store shape, optional callback, raw/native failures, undefined abort default and isolated listener lifetime. |
| crates/pi-ai/tests/{auth,auth_resolve_contract,models,models_contract,provider_auth,differential_providers,lossless_auth_interaction}.rs | Auth values, callback/request consumers and native UTF-16. |
| crates/pi-coding-agent/tests/{auth_storage,auth_storage_cross_process,runtime_credentials}.rs | Backend rejection, persistence, lock protocol and runtime override behavior. |

## Known gaps (current cycle)

No open interface gap after the recorded exclusions. Separate module rows and parent milestone verification remain independent work.

## Out of scope

- JS-only non-JSON callback rejections: Rust-native extensions (2026-10-18) plus typed-core invalid-JS-runtime-type exclusion (2026-08-12), reaffirmed2026-10-24. Native callbacks use shared CallbackError/AbortReason; no new carrier.
- Invalid typed record shapes and JS mutation-through-returned-reference, under existing typed/owned-value decisions. Environment/header key domains follow [lossless-content](lossless-content.md).
- Whole helper/context/OAuth implementation revalidation and queued old-only OAuth/load deletion; tracked in their own source rows.
