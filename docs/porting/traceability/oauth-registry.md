# Frozen OAuth registry and result provenance

## Successor audit in progress (2026-10-25)

Public compile RED (`/tmp/ai-successor-public-red.log`) identifies absent direct helpers, builtin objects, method constants and synchronous callback rejection results. The public barrel now exposes each frozen entry (Rust builtin objects use Arc-returning accessors); all helpers call the existing native flows, not duplicate credential constructors. Legacy adapters retain Copilot allowEmpty, cancel empty/absent selections, and wait for the browser when manual input is absent. Synchronous auth/device/progress callbacks now return Result; CLI, interactive, two extension examples and their fixtures receive only mechanical propagation updates. Frozen credential/key-order code is untouched.

Response-boundary RED (`/tmp/ai-successor-body-manual-red.log`): successful Codex body failures were wrongly caught as fetch failures; non-2xx body failures bypassed the frozen status-text policy; Anthropic JSON failures used Error instead of SyntaxError; non-Error manual rejections stayed raw. Codex now catches only fetch at its source fetch catch, reads bodies at the operation boundary, preserves successful JSON/body throws, and catches error-body reads only where frozen does. Browser exchanges no longer inherit a caller abort signal absent from source. Shared native manual normalization and JSON-error authority retain UTF-16; device-auth strings also remain native through HTTP JSON/form boundaries.

Native regression `oauth_repin_edges` reproduced seven real common-pin differences (all seven failed before production edits): first-value query/hash parsing, nonempty Copilot proxy matching, Anthropic/Codex empty-manual second prompts, exact polling rejection propagation, Codex non-Error refresh formatting, and SyntaxError identity. Shared parser/response helpers and both browser flows now follow those literal source branches. RED log: `/tmp/ai-successor-edges-red.log`. Whole rows remain open until the remaining exports, flow audit and verification finish. Follow-up native RED proves Codex expiry used the pre-request clock (61,000 vs65,000) and Copilot forwarded a login signal to the device request where frozen sends none; `/tmp/ai-successor-clock-signal-red.log`. Repairs read the clock after response decoding and restrict Copilot HTTP signals to its independent model-list deadline.

Source `packages/ai/src/utils/oauth/index.ts`, `types.ts`, public `oauth.ts` at `4f9e6f8a4`.
Rust `crates/pi-ai/src/oauth.rs`, `oauth_registry.rs`.

Current auth continuation and remaining boundaries: [auth integration status](auth-storage.md#integration-status). No registry or provider module is promoted to whole-module parity.

| Source surface | Native mapping | Proof / status |
|---|---|---|
| Provider interface/id/info/callbacks/credentials | `OAuthProviderInterface`, aliases, existing callback trait | fake-provider public tests; typed Rust excludes invalid JS values |
| Builtin insertion order | five frozen IDs, Codex GC/Team distinct identity, frozen names | tracked registry test proves IDs/order; external synthetic builtin probe proves names/mappings, `/tmp/pi-rust-repin-auth/VERIFY-PARTIAL.md` |
| get/register/unregister/reset/getAll/info | same-ID replacement retains slot; unregister restores builtin or removes custom | registry test; extended restore/order mutation proof pending |
| refreshOAuthToken | lookup, unknown-provider failure, direct provider result | registry test |
| getOAuthApiKey | lookup first; missing credentials returns None; refresh only expired; wrap refresh errors; derive key then return credentials | registry fake-provider tests plus coding-agent public rotation/delta/lossless proofs; see [auth trace](auth-storage.md) |
| provider getApiKey | access string | external synthetic builtin mapping probe; no live provider/refresh proof |
| Copilot modifyModels | preserve other providers, filter account model IDs when present, derive URL from token/domain; native result is now Result<Vec<Model>, CallbackError>, with success-only builtin output wrapped in Ok | external synthetic 20-combination builtin probe; not login/refresh protocol proof |
| built-in login/refresh flows | existing native OAuthAuth flow bridge | NOT revalidated at common pin; upstream provider-module slices own protocol/callback deltas |
| direct provider helper exports (loginAnthropic etc.) | existing old modules, not frozen public exports | pending provider-module port; not claimed complete |

Parent-approved2026-10-24: model modifiers use the existing lossless CallbackError authority instead of an infallible return. Default and builtin transforms return Ok; registry/custom modifiers propagate rejection unchanged. [Registry entrypoint and consumer proof](model-registry.md#modifier-failure-consumer-boundaries-approved-migration-in-progress) tracks the active migration; no new source deviation or alternate API.

The external probe covers five builtin IDs/names/callback flags × four synthetic credential fixtures (20 mapping combinations), not registry mutation/error coverage or provider protocols. `modify_models` is an identity default on every Rust provider, whereas frozen TS exposes optional `modifyModels` only on Copilot; valid mapped outputs do not establish optional-method shape parity.

The old Copilot native-flow bridge permits policy-enabled models when no picker ID survives; frozen selection requires `model_picker_enabled === true`. This is a source-inspection discrepancy, not an executed refresh proof or approved deviation. Callback translation through old `AuthPrompt` likewise leaves lifecycle/cancellation/protocol parity pending; see [verification discrepancies](/tmp/pi-rust-repin-auth/VERIFY-PARTIAL.md).

No real providers, credentials, login, or network used. Builtin flow bridge is compilation/integration scaffolding only, not current-pin provider-flow parity proof.

Q18 native auth integration and bounded proof status: [lossless auth](lossless-auth.md).
