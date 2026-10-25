# OpenAI Codex OAuth — common-pin audit

Whole `packages/ai/src/utils/oauth/openai-codex.ts` at `4f9e6f8a4c0065d81b10e7177b28d5082f0bc30d` audited 2026-10-25. Native: `crates/pi-ai/src/auth/oauth/openai_codex.rs`; legacy bridge in `oauth_registry.rs`. [Contract](../../specs/oauth-common-pin.md), [final ledger](../ai-core-successor-proof.md).

## Whole-module branch audit

Constants match client ID, auth/authorize/token/device URLs, localhost1455/auth/callback browser redirect, device redirect, verification URL, 900-second device deadline, browser/device_code method IDs, scope and JWT claim path. Callback host uses the setting/default. Browser createAuthorizationFlow generates PKCE then16-byte hex state, preserves query insertion order/flags and accepts optional native originator. Node dynamic crypto/http readiness is a runtime exclusion, not a fallback.

Authorization input shares the exact trim/URL/first-query/hash/query-string/raw rules with Anthropic. decodeJwt accepts exactly three segments and standard atob (including its whitespace behavior, rejecting base64url '-'/'_'), interprets bytes as Latin-1 code units, then JSON.parse or null on failure. getAccountId requires a nonempty string; getEmail uses nonempty direct email before profile.email. Existing credentialsFromToken and OAuthCredential key-order code are unchanged: access/refresh/expires/accountId and optional email retain the existing authority.

fetchWithLoginCancellation catches only fetch rejection, replacing it with Login cancelled only when its supplied signal is aborted; body/JSON failures occur outside that catch. readTokenResponse catches non-2xx response.text failures as empty text, uses body or statusText in the exact operation diagnostic, otherwise reads JSON and validates required fields. Date.now is sampled after the response is decoded. exchangeAuthorizationCode preserves form key order/client/verifier/redirect and browser sends no signal; device exchange sends the device signal. refreshAccessToken wraps only fetch failures with Error.message or String(value); response/body/JSON failures keep their separate source policy.

startOpenAICodexDeviceAuth sends JSON client ID and signal; 404 has its dedicated message; other error bodies are read with source catch; valid response requires device/user code and finite nonnegative interval, including Number(string.trim()). Device IDs/codes/verifiers remain native strings through JSON/form boundaries. pollOpenAICodexDeviceAuth preserves successful JSON rejection, success fields, unconditional403/404 pending, parsed direct/nested pending/slow_down errors, and exact other status/body failure text. Fetch rejection preserves its original identity after the cancellation catch rather than becoming a Failed.message. Device login notifies code/URL/interval/900seconds, polls, then exchanges with the device redirect.

startLocalOAuthServer returns the exact404/400/200 shared pages, verifies state before nonempty code, settles once, and degrades failed binding to no listener/manual flow as frozen does. Browser login invokes onAuth before try/finally, races optional manual input and browser callback, wraps non-Error manual rejection, rejects nonempty mismatched state, and uses the second text prompt if no code exists. Missing manual callback is distinct from pending manual input, including after failed binding. The listener closes before awaiting token exchange; the modern wrapper aborts the manual prompt only when the whole login finishes. Browser flow ignores the caller signal because the frozen browser utility has no signal option.

Modern openaiCodexOAuth selects browser/device, rejects unknown methods, adds the existing oauth type marker, refreshes through the existing credentials authority and returns access as API key. Legacy provider selection also treats empty/absent selection as Login cancelled. Three provider IDs/names remain distinct, all callback-server flags=true, sharing the same login/refresh/access behavior. Direct browser/device functions bypass selection; public method constants/getters are present.

## Source discrepancy preserved

DISCREPANCY: onAuth runs before the browser utility's server-close finally. If it throws, the callback listener leaks; outer modern auth still aborts its manual signal and returns the exact rejection. Existing `auth_types_contract::codex_auth_url_throw_preserves_frozen_listener_leak` checks the real occupied port1455 in isolated processes and retains the actual-source proof. This was not silently repaired.

## Proof and persistence

Existing native browser callback/device/refresh/JWT/interval/error/toAuth tests pass. `differential_oauth_flows` compares actual frozen/native device, form exchange, refresh credentials and requests. `auth_codex_credential_order` passes all three tests, including seven actual frozen AuthStorage/Codex wrapper cases covering absent/direct/empty/profile/precedence/undecodable/native-unit email and exact auth.json bytes. No credentialsFromToken/key-order reimplementation occurred.

New native regressions prove raw polling failures, String(false) refresh formatting, successful/non-2xx body-failure catch boundaries, SyntaxError identity, post-response expiry, empty/manual/absent callback behavior, empty callback code rejection and listener close before a blocked token result. Exact commands/revisions/results are in the [checkpoint](../ai-core-successor-proof.md).

Gap: wrong-typed/exotic token/callback shapes, browser/Node/crypto readiness, numeric extremes and Node/V8 scheduling/runtime faults use the standing exclusions; existing native string/JSON/credential carriers remain exact. No live Codex calls or new frozen differential/matrix.
