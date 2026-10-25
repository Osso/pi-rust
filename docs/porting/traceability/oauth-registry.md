# OAuth public entry, registry and callback types

Whole-module audit: frozen `packages/ai/src/oauth.ts` (one wildcard export), `utils/oauth/index.ts` and `utils/oauth/types.ts` at `4f9e6f8a4c0065d81b10e7177b28d5082f0bc30d`, completed 2026-10-25. Native: `crates/pi-ai/src/{oauth,oauth_registry}.rs`, `compat/extension_oauth_types.rs`. [Contract](../../specs/oauth-common-pin.md), [final proof ledger](../ai-core-successor-proof.md).

## Public exports and types

The root forwards the entire registry entry. Anthropic login/refresh/provider, Copilot login/refresh/provider/domain/base-URL helpers, Codex browser/device login/refresh/three providers/two method constants, and device-code exports are present. Frozen constant provider objects use Arc-returning native accessors that select the immutable builtin, independent of registry overrides. The public-native compile/behavior regression reproduced absent exports before repair and exercises getters, constants, direct prompt/notify rejection and all three Codex cancellation routes.

OAuthCredentials aliases the existing OAuthCredential authority: refresh/access are native strings, expires is a JS number, extra fields and original key order remain on the existing carrier. No constructor/serialization/key-order rewrite occurred here. OAuthProviderId and deprecated OAuthProvider are scalar IDs. OAuthPrompt/AuthInfo/DeviceCodeInfo/SelectOption/SelectPrompt preserve every required/optional field, native message/URL/code/id/label strings and fractional interval/expiry values. OAuthProviderInfo exposes id/name/available; all returned info rows are available=true.

OAuthLoginCallbacks requires auth/device/prompt/select; progress/manual/signal remain optional. Synchronous auth/device/progress callbacks return Result to represent source throws without panics. The legacy adapter preserves Copilot allowEmpty, native notifications, exact rejection values and optional signals; empty/absent selections mean Login cancelled. An omitted manual callback is distinct from a pending callback: it must neither prompt immediately nor block the text prompt after Codex bind failure. The optional manual bridge on AuthInteraction defaults to the native auth wrapper's required manual prompt and is overridden only by the legacy adapter. This models an actual source option, not a runtime gap.

Provider login/refresh/getApiKey and optional callback-server flag map to the five builtin identities/names/flags. Only Copilot rewrites models: unrelated records survive untouched; present availableModelIds filters by exact membership (including empty array), absent IDs retain the catalog, and baseUrl follows token/domain/default precedence. The trait's default identity modifier represents absent optional modifiers; no JS method-reflection API is claimed. Returned values follow the approved owned-value rule.

## Registry and result branch audit

The insertion-ordered vector matches Map: replacement retains the existing slot, a new custom ID appends, unregister restores the original builtin in its slot or removes a custom ID, reset restores the five original IDs in frozen order, and getters/info preserve that order. Code paths were compared in full; the existing standalone registry test exercises insertion/order, refresh/no-refresh, missing credentials, removal and unknown-provider text. No new mutation matrix was introduced.

refreshOAuthToken looks up before calling and returns the provider result unchanged. getOAuthApiKey looks up before checking credentials, returns None only for a missing credential, refreshes on now>=expires, wraps only refresh failures with the exact shared Error-or-String policy plus original cause, then derives the API key and returns the updated credential. Existing `oauth_abort_wrapper` imports the real frozen helper and proves raw/default/custom abort values and error causes. Fake-provider tests and app auth-storage tests retain refresh/persistence proof.

Legacy Copilot refresh passes its stored enterpriseUrl directly, whereas the modern auth wrapper normalizes it; the bridge keeps these separate. Builtin protocol/callback behavior is owned by the [Anthropic](anthropic.md), [Copilot](auth-oauth-github-copilot.md) and [Codex](openai-codex.md) module audits, not inferred from builtin mappings.

## Proof and exclusions

Current evidence: `public_oauth_types`, `oauth_registry`, `oauth_abort_wrapper`, `lossless_oauth_callbacks`, `auth_types_contract` and the 20 native `oauth_repin_edges` regressions; exact commands/revisions/results are in the [successor checkpoint](../ai-core-successor-proof.md). The public-export RED log is `/tmp/ai-successor-public-red.log`; optional-manual RED is `/tmp/ai-successor-manual-absent-red.log`. No live OAuth/provider calls, new frozen differential or new matrix.

Credential HTTP/persistence proof is retained: [auth integration](auth-storage.md), [lossless auth](lossless-auth.md), existing actual-source OAuth flow requests and app `auth_codex_credential_order` compare exact persisted shape/order. Native scalar IDs and owned returns follow AGENTS.md; historical source paths are not current proof.

Gap: wrong-typed/exotic credentials or JS reflection/object identity, scalar-name lone surrogates beyond existing carriers, clock extremes and Node/V8/runtime scheduling oddities follow the standing exclusions; existing native credential/callback carriers remain exact.
