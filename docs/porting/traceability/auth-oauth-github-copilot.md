# GitHub Copilot OAuth — common-pin audit

Whole `packages/ai/src/utils/oauth/github-copilot.ts` at `4f9e6f8a4c0065d81b10e7177b28d5082f0bc30d` audited 2026-10-25. Native: `crates/pi-ai/src/auth/oauth/github_copilot.rs`; legacy modifier/callback bridge in `oauth_registry.rs`. [Contract](../../specs/oauth-common-pin.md), [final ledger](../ai-core-successor-proof.md).

## Whole-module branch audit

Client ID, four Copilot headers, API version2026-06-01 and all three domain-derived device/access/Copilot token endpoints match frozen. normalizeDomain trims, accepts a URL or bare hostname, returns parsed hostname or None. getBaseUrlFromToken uses the first nonempty proxy-ep match and replaces only leading proxy.; empty matches do not become https://. Token URL wins over nonempty enterprise domain; empty/absent domain uses api.individual.githubcopilot.com. Realistic URL/form/header conversions occur at existing HTTP boundaries.

Model availability requires data array and string ID, model_picker_enabled===true, policy state not disabled, and supports.tool_calls not false. IDs retain source order and duplicates. Removed the old policy-enabled fallback. The full flow parses HTTP JSON through JsValue, so ignored unknown properties cannot force a scalar projection; model IDs follow the approved scalar-ID boundary. Available model request sends Accept/Bearer/four standard headers/API version and only its independent AbortSignal.timeout(5000).

fetchJson propagates fetch/body failures unchanged, formats status/statusText/text for non-2xx and preserves JSON.parse SyntaxError identity. startDeviceFlow posts the ordered URLSearchParams client/scope request; validates fields/optional numeric interval; accepts only normalized HTTP(S) verification URLs before notifying. Device code/user code remain JsString until form/notification boundaries. Source-like malformed runtime object shapes use the standing exclusion, not fallback behavior.

pollForGitHubAccessToken waits before first poll and uses the shared source poll helper. Request body order is client/device/grant; the login signal is checked by the poll helper, not forwarded into fetch. Complete access_token (including empty string), pending, slow_down with optional interval, failed description suffix and invalid response branches match source. Fetch/body/JSON rejection stays the exact rejection rather than becoming a new message-only Failed result. Existing paused-clock tests prove waits, slow_down and timeout.

refreshGitHubCopilotAccessToken sends Accept/Bearer/standard headers, applies expires_at*1000 minus five minutes, retains the original refresh token and optional enterpriseUrl. refreshGitHubCopilotToken then appends availableModelIds. Login prompts enterprise domain (legacy allowEmpty=true, modern text prompt omits that field), checks cancellation after the prompt, rejects invalid nonempty domains, starts/notifies/polls, gets the Copilot token, emits Enabling models..., enables all catalog models concurrently, then fetches account availability. No start/poll/refresh/policy HTTP request inherits a login signal. Policy requests use ordered policy body/headers, catch every fetch failure, and never consume response bodies; source Promise.all starts all28 together.

The modern githubCopilotOAuth wrapper normalizes stored enterpriseUrl on refresh/toAuth and derives per-credential baseUrl. The legacy provider's refresh passes enterpriseUrl directly. Its modifier preserves unrelated providers, filters only when availableModelIds exists (empty list removes all Copilot models), and rewrites baseUrl; no modifier exists on the modern Models/Provider API. Legacy/native public helpers and names/flags are mapped in the [registry audit](oauth-registry.md).

## Existing and native proof

The current actual-source `differential_github_copilot_oauth` refresh/toAuth cases compare complete credentials and HTTP requests; the existing native suite includes account picker predicates, URL security, policy concurrency, polling deadlines and the imported login policy-concurrency oracle. `legacy_modify_models_filters_present_ids_and_preserves_absent_id_catalog` compares complete frozen/native records including native display text and unrelated models. The independent model timeout source test retains TimeoutError/code23.

New native regressions reproduce empty proxy matches, signal forwarding, raw poll rejection replacement, SyntaxError brand, empty enterprise endpoint and policy-body consumption. The last test observed28 unwanted body reads before repair. These are fixes to ordinary source branches, not new frozen differentials/matrices. Full commands/revisions/results are in the [checkpoint](../ai-core-successor-proof.md).

Gap: wrong-typed/exotic JSON or metadata, IDNA edge cases, numeric extremes and Node/V8 timer/network scheduling or fault shapes use the standing exclusions; existing lossless carriers remain exact. No live Copilot calls.
