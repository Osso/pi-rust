# auth/oauth/anthropic traceability

Pinned source: `packages/ai/src/auth/oauth/anthropic.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| Constants | Client id, authorize/token URLs, callback host env (`PI_OAUTH_CALLBACK_HOST`), port 53692, `/callback`, localhost redirect, scopes. | Private constants in `anthropic`; `manual_code_login_posts_the_expected_exchange_body_and_cancels_the_prompt`, `callback_login_uses_the_local_server_and_redirect_uri`. |
| `parseAuthorizationInput` | Trim; URL query `code`/`state`; `code#state`; query-string form; otherwise whole input as code. | `support::parse_authorization_input`; `manual_state_mismatch_fails_with_the_source_message` and differential manual flow. |
| `formatErrorDetails` | `Name: message`, optional code/errno/cause/stack. | `format_error_details` emits `Error: {message}` (typed errors carry no code/errno/stack); `token_exchange_http_failures_keep_the_wrapped_details`. |
| `startCallbackServer` | Bind `host:53692`; 404 unknown path; 400 error/missing/state mismatch; 200 success page; settle code once. | `start_anthropic_callback_server` + `support::start_callback_server`; callback test asserts status/body. |
| `postJson` | POST JSON, 30 s timeout, non-2xx error text. | `post_json`; timeout maps to the fetch abort message. |
| `exchangeAuthorizationCode` | Body order `grant_type, client_id, code, state, redirect_uri, code_verifier`; wrapped request/JSON errors; expiry `now + expires_in*1000 - 5min`. | `exchange_authorization_code`; differential flow compares body, credential, and expiry. |
| `loginAnthropic` | PKCE; state = verifier; notify auth URL then progress; manual prompt races callback; manual state check; missing code/state errors; close server and abort prompt signal. | `login_anthropic`; manual + callback tests; differential manual and callback scenarios compare events/prompts/requests/callback page. |
| `refreshAnthropicToken` | JSON refresh without `scope`; wrapped errors; 5 min skew. | `refresh_anthropic`; `refresh_omits_scope_and_applies_the_five_minute_skew`; differential refresh. |
| `anthropicOAuth` | Name, subscription flag, `toAuth` → access token as api key. | `AnthropicOAuth`; `to_auth_returns_the_access_token_and_marks_subscription_login`. |

## Exclusions and discrepancies

- Token responses are validated as typed fields; the source would return `undefined` access/refresh/expires on malformed JSON shapes. Rust returns a `... response missing ...` error.
- Error details omit `code`, `errno`, `cause`, and `stack`; `CallbackError` has only a message.
- The source's Web Crypto / `node:http` runtime checks have no Rust equivalent; the callback server is always available and bind failures surface as login errors.
- Differential: `oracle/oauth-flows-oracle.ts` patches `Date.now`, `crypto.getRandomValues`, `crypto.randomUUID`, and `node:crypto.randomBytes`; PKCE/state therefore compare exactly. Callback pages are compared byte-for-byte.
