# auth/oauth/openai-codex traceability

Pinned source: `packages/ai/src/auth/oauth/openai-codex.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| Constants | Client id, auth URLs, localhost:1455 `/auth/callback`, device endpoints, 900 s device timeout, browser/device method ids, JWT claim path. | Private constants in `openai_codex`; `device_code_login_notifies_polls_and_extracts_the_account_id`, `browser_login_hits_the_local_callback_and_uses_the_browser_redirect_uri`. |
| `createState` | 16 random bytes hex. | `create_state` via `support::random_hex`; differential device flow. |
| `parseAuthorizationInput` | Shared manual-input parser. | `support::parse_authorization_input`. |
| `decodeJwt` / `getAccountId` | Three-part token, standard Base64 payload, `https://api.openai.com/auth.chatgpt_account_id` non-empty. | `support::decode_jwt_payload`, `get_account_id`; differential compares `accountId`; `rejects_unknown_login_methods_and_missing_account_ids`. |
| `fetchWithLoginCancellation` | Cancellation → `Login cancelled`. | `fetch_with_login_cancellation`. |
| `readTokenResponse` | Non-2xx text/statusText error; missing fields error with JSON; expiry `now + expires_in*1000`. | `read_token_response`; differential exchange/refresh. |
| `exchangeAuthorizationCode` / `refreshAccessToken` | Form bodies and Content-Type; refresh error prefix. | `exchange_authorization_code`, `refresh_access_token`; differential. |
| `startOpenAICodexDeviceAuth` | JSON client id; 404 special message; string/number interval validation. | `start_device_auth`; `device_code_404_has_the_source_message`; differential device. |
| `pollOpenAICodexDeviceAuth` | 200 success; 403/404 pending; JSON `error.code` pending/slow_down; other failures. | `poll_device_auth`; differential pending-free success and behavioral suite. |
| `createAuthorizationFlow` | PKCE + state; authorize query order and flags (`originator=pi`). | `login_browser` URL construction; differential device does not exercise browser URL; behavioral callback test asserts `originator=pi`. |
| `startLocalOAuthServer` | Port 1455; 404/400/200 pages; bind failure degrades to manual-only. | `start_local_server`; callback test. |
| `loginOpenAICodex` | Manual prompt races callback; state mismatch `State mismatch`; missing code. | `login_browser`; behavioral callback/manual paths. |
| `loginOpenAICodexDeviceCode` | Device event then poll then exchange with device redirect. | `login_device_code`; differential device. |
| `login` select | Browser default/device options; unknown method error. | `login_openai_codex`; `rejects_unknown_login_methods_and_missing_account_ids`. |
| `openaiCodexOAuth` | Name, subscription, `toAuth` access token. | `OpenAICodexOAuth`; behavioral test. |

## Exclusions and discrepancies

- `readTokenResponse` uses `HttpResponse::status_text` for an empty body. Invalid JSON is reported as a missing-fields error rather than the JS `SyntaxError` text.
- Token responses are typed; malformed responses produce Rust errors instead of `undefined` credential fields.
- Differential normalizes nothing for this flow: fixed clock and deterministic randomness make state/PKCE/expiry identical.
