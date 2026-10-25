# Anthropic OAuth — common-pin audit

Whole `packages/ai/src/utils/oauth/anthropic.ts` at `4f9e6f8a4c0065d81b10e7177b28d5082f0bc30d` audited 2026-10-25. Native: `crates/pi-ai/src/auth/oauth/{anthropic,support}.rs`; public legacy bridge in `oauth_registry.rs`. [Contract](../../specs/oauth-common-pin.md), [final proof ledger](../ai-core-successor-proof.md).

## Whole-module branch audit

Constants match the decoded client ID, claude.ai authorize URL, platform.claude.com token URL, callback host setting/default, fixed port53692/path /callback, localhost redirect URI and the entire scope string. PKCE verifier is the expected state; authorize parameters preserve frozen order and URLSearchParams encoding. Browser/Node import readiness is a runtime exclusion, not an alternate provider implementation.

parseAuthorizationInput follows trim, absolute URL query, first two hash segments, query-string form and raw code precedence. URLSearchParams.get takes the first duplicate key. Native regression reproduced last-value and extra-hash drift; native string parsing retains code/state until the URL/HTTP boundary. State checks ignore empty state where source uses truthiness; an explicitly empty resolved state still reaches Missing OAuth state.

The callback listener returns exact shared error/success pages: unknown path404; nonempty OAuth error400 without settling; missing/empty code or state400; mismatched state400; valid code/state200 and settle once. Empty error is ignored. Callback query/header/runtime HTTP oddities are excluded; ordinary occupied-port rejection now has Node listen message/code/errno/syscall/address/port, proven with a real occupied loopback listener.

postJson sends POST Content-Type/Accept and ordered JSON, reads the text body, uses only its own 30,000ms timeout signal, and creates the exact non-2xx diagnostic. exchangeAuthorizationCode catches request rejection separately from JSON.parse failure, preserving native error details; credentials use refresh/access/expires with Date.now after response and the five-minute skew. refreshAnthropicToken sends refresh grant/client/refresh token, applies the same separate catches/skew and omits scope.

formatErrorDetails preserves name/message, truthy code, present errno and recursively branded causes using the existing NativeNodeError::cause authority. Non-Errors use the shared String(value) conversion. V8 stack omission is approved. Native RED showed branded Error causes becoming [object Object]; the regression now proves TypeError→Error/code/errno recursion. Existing seven-case source proof still covers raw objects, false/null causes, errno0, native units and conversion errors.

loginAnthropic starts the listener before onAuth, then races optional manual input with callback completion. Manual Error rejections keep their identity; other rejections become Error(String(value)). Manual state mismatch fails with OAuth state mismatch. Empty/manual input with no code uses the second text prompt and exact placeholder; missing code/state errors remain exact. No manual callback means wait for callback, not an immediate text prompt. onProgress occurs before exchange. The listener closes before awaiting the returned exchange promise; outer modern auth finally aborts the manual prompt only after completion/rejection. Browser helpers do not inherit a caller cancellation signal absent from source.

anthropicOAuth exposes the frozen name, login/refresh with the existing tagged credential authority and toAuth(access). anthropicOAuthProvider exposes id/name/callback flag, direct callbacks/refresh/getApiKey with no extra selection. The shared legacy bridge retains optional manual presence and Result notification failures. No credential/key-order reimplementation.

## Callback close final-gate repair (2026-10-25)

The shared listener from `3e6438e6e` lived inside the spawned accept future. `CallbackServer::close` aborted that task but did not release its listening fd until the task was scheduled/dropped, allowing the next login to encounter EADDRINUSE. A high-concurrency composed run exposed that occupied-port failure; the original AuthUrl timeout did not record the login task's result, so linking that first timeout to the port race remains an inference. Thirteen default pre-fix composed runs passed.

Frozen Anthropic `finally` at line343 and Codex `finally` at line568 call Node `server.close()` synchronously before returning exchange work. A direct Node close→same-port bind succeeds. Native `closing_callback_server_releases_its_port_without_a_scheduler_turn` reproduced EADDRINUSE deterministically before the fix and passes after it, both before and after the acceptor is polled. The shared runtime now owns the listener outside the task; each accept poll borrows it under a mutex, and close removes it synchronously before aborting the task. No API, callback order, existing assertion, or timeout changed. Logs: `/tmp/pi-repin-final-gate/{callback-close-red,callback-close-green,node-callback-close-proof}.log`. Repair commit `484d9ec9c` passes strict pi-ai clippy and the workspace all-target check. Post-commit full AI tests pass (902); five composed repeats pass (401 each), and three callback contention repeats pass (21 each). See `/tmp/pi-repin-final-gate/{ai-tests-final,ai-composed-fixed-*,ai-callback-contention-fixed-*}.log`.

## Proof

Existing standalone `anthropic_oauth` six tests, current actual-source `differential_oauth_flows` manual/callback/refresh HTTP+credential/page comparisons, `differential_anthropic_timeout` deadline and error-format cases, `auth_types_contract` notification failures, and `oauth_registry` native token/header/form tests pass. Native `oauth_repin_edges` covers parser/manual/JSON/cause/callback/bind/listener lifetime discrepancies found in this audit. Exact commands and immutable code revision are in the [checkpoint](../ai-core-successor-proof.md).

Gap: malformed/wrong-typed token JSON, adversarial/exotic callback inputs, browser/Node/crypto readiness, extreme numbers and runtime scheduling/fault shapes follow AGENTS.md; existing lossless strings/errors stay native. No live provider calls or new frozen differential/matrix.
