# auth/oauth/anthropic traceability

Q18 human-error representation and bounded proof: [lossless provider errors](lossless-provider-errors.md). Earlier counts are historical, not rerun evidence.

Current reason/deadline proof imports `packages/ai/src/utils/oauth/anthropic.ts` at common pin `4f9e6f8a4`; older full-flow evidence below remains historical.

## Reason-bearing request deadline

`post_json` passes only its independent shared 30,000ms timeout signal through fetch/body, not the Rust trait's caller signal. `format_error_details` consumes `CallbackError::thrown_value`: native Error/DOMException name/message, truthy code, present errno and raw cause; non-Errors use native String conversion, including conversion failures. V8 stacks are omitted by approved decision. Recursive branded Error causes are not yet representable in NativeNodeError's raw JSON metadata and remain partial, never inferred from plain name/message objects.

Actual frozen refresh helper RED→GREEN: `differential_anthropic_timeout` tests native TimeoutError/code23 and seven Error/value format cases (native UTF-16, raw object lookalike, non-callable toString TypeError, errno zero/null and false/plain-object cause). Source oracle records requested 30,000ms then accelerates only the builtin clock to1ms; Rust advances virtual time. Logs `anthropic-timeout-red-2.log`, `anthropic-reason-format-red.log`, `anthropic-timeout-green-2.log` under `/tmp/pi-rust-repin-delta-ai`. Full OAuth malformed token/runtime behavior is not proven by this slice.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| Constants | Client id, authorize/token URLs, callback host env (`PI_OAUTH_CALLBACK_HOST`), port 53692, `/callback`, localhost redirect, scopes. | Private constants in `anthropic`; `manual_code_login_posts_the_expected_exchange_body_and_cancels_the_prompt`, `callback_login_uses_the_local_server_and_redirect_uri`. |
| `parseAuthorizationInput` | Trim; URL query `code`/`state`; `code#state`; query-string form; otherwise whole input as code. | `support::parse_authorization_input`; `manual_state_mismatch_fails_with_the_source_message` and differential manual flow. |
| `formatErrorDetails` | `Name: message`, optional code/errno/cause/stack. | `format_error_details` uses branded rejection and native metadata; `differential_anthropic_timeout` covers deadline + seven rejection cases. Recursive Error causes remain partial; stacks omitted by approval. |
| `startCallbackServer` | Bind `host:53692`; 404 unknown path; 400 error/missing/state mismatch; 200 success page; settle code once. | `start_anthropic_callback_server` + `support::start_callback_server`; callback test asserts status/body. |
| `postJson` | POST JSON, 30 s timeout, non-2xx error text. | `post_json`; shared independent timeout signal exposes TimeoutError DOMException; differential deadline test. |
| `exchangeAuthorizationCode` | Body order `grant_type, client_id, code, state, redirect_uri, code_verifier`; wrapped request/JSON errors; expiry `now + expires_in*1000 - 5min`. | `exchange_authorization_code`; differential flow compares body, credential, and expiry. |
| `loginAnthropic` | PKCE; state = verifier; notify auth URL then progress; manual prompt races callback; manual state check; missing code/state errors; close server and abort prompt signal. | `login_anthropic`; manual + callback tests; differential manual and callback scenarios compare events/prompts/requests/callback page. |
| `refreshAnthropicToken` | JSON refresh without `scope`; wrapped errors; 5 min skew. | `refresh_anthropic`; `refresh_omits_scope_and_applies_the_five_minute_skew`; differential refresh. |
| `anthropicOAuth` | Name, subscription flag, `toAuth` → access token as api key. | `AnthropicOAuth`; `to_auth_returns_the_access_token_and_marks_subscription_login`. |

## Exclusions and discrepancies

- Token responses are validated as typed fields; the source would return `undefined` access/refresh/expires on malformed JSON shapes. Rust returns a `... response missing ...` error.
- Error details now preserve code/errno/raw cause and names; recursive Error cause brands remain partial. Stack omission follows the approved general rule.
- The source's Web Crypto / `node:http` runtime checks have no Rust equivalent; the callback server is always available and bind failures surface as login errors.
- Differential: `oracle/oauth-flows-oracle.ts` patches `Date.now`, `crypto.getRandomValues`, `crypto.randomUUID`, and `node:crypto.randomBytes`; PKCE/state therefore compare exactly. Callback pages are compared byte-for-byte.
