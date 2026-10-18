# auth/oauth/openrouter traceability

Pinned source: `packages/ai/src/auth/oauth/openrouter.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| Constants | Authorize/token URLs, 5 min login timeout, 30 s exchange timeout. | Private constants in `openrouter`. |
| `parseAuthorizationInput` | URL/query `code`, else trimmed input. | `parse_authorization_input`; `manual_codes_are_trimmed_and_exchanged_without_a_callback`, `empty_manual_input_fails_without_a_token_request`. |
| `errorDetail` | `error_description`, `message`, string `error`, `error.message`. | `error_detail`; `callback_exchange_failures_reach_the_page_and_the_login`. |
| `exchangeAuthorizationCode` | Abort → `Login cancelled`; timeout message; JSON `{code, code_verifier, code_challenge_method}`; invalid JSON; HTTP detail; missing key; permanent credential (`refresh:""`, `MAX_SAFE_INTEGER`). | `exchange_authorization_code` / `credential_from_response`; differential manual + callback + exchange-failure test. |
| `startCallbackServer` | Ephemeral loopback port; 404 method/path; 409 already used; error/missing-code pages; exchange in handler; 200/502; cache-control no-store. | `start_callback` + `support::start_callback_server`; callback tests. |
| `loginOpenRouter` | UUID callback path; progress + auth URL; manual prompt races callback; timeout; exchange progress; prompt signal cancelled; server closed. | `login_openrouter`; behavioral + differential manual/callback. |
| `openRouterOAuth` | Name, login label, no subscription, identity refresh, `toAuth` key. | `OpenRouterOAuth`; `refresh_is_identity_and_to_auth_uses_the_key`. |

## Exclusions and discrepancies

- The ephemeral port is inherently non-deterministic; differential comparison normalizes `127.0.0.1:<port>` and its URL-encoded form.
- The 409 one-shot branch is implemented but not covered by a dedicated deterministic test; the callback tests cover the claimed path.
- `Response.json()` failure on a non-OK response yields an empty body object in Rust (`Value::Null`) rather than a thrown parse error.
