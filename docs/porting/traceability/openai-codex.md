# auth/oauth/openai-codex traceability

## Q15c current-pin notifier boundary

Current authority: `packages/ai/src/utils/oauth/openai-codex.ts` at4f9e6f8a4. Source:490 emits onAuth before try/finally493–569. **DISCREPANCY preserved:** notify throws propagate unchanged and leak the browser callback listener; the outer auth wrapper still aborts manual input. Real isolated-process RED/GREEN compares the thrown false, select/auth_url ordering and an occupied port1455 after rejection. Test/oracle process exit releases listeners. `q15c-auth-errors-and-discrepancy-green.log`; remaining whole-module proof is not established by this branch.

Historical pre-repin mapping below:

Pinned source: `packages/ai/src/auth/oauth/openai-codex.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| Constants | Client id, auth URLs, localhost:1455 `/auth/callback`, device endpoints, 900 s device timeout, browser/device method ids, JWT claim path. | Private constants in `openai_codex`; `device_code_login_notifies_polls_and_extracts_the_account_id`, `browser_login_hits_the_local_callback_and_uses_the_browser_redirect_uri`. |
| `createState` | 16 random bytes hex. | `create_state` via `support::random_hex`; differential device flow. |
| `parseAuthorizationInput` | Shared manual-input parser. | `support::parse_authorization_input`. |
| `decodeJwt` / `getAccountId` | Three-part token, payload decoded with `atob` semantics (`pi_js_compat::base64::atob`: standard alphabet only, whitespace ignored, `-`/`_` rejected), `https://api.openai.com/auth.chatgpt_account_id` non-empty. | `support::decode_jwt_payload`, `get_account_id`; differential compares `accountId`; `rejects_unknown_login_methods_and_missing_account_ids`, `base64url_jwt_payloads_fail_like_atob` (base64url payload reaches `Failed to extract accountId from token`). |
| `fetchWithLoginCancellation` | Cancellation → `Login cancelled`. | `fetch_with_login_cancellation`. |
| `readTokenResponse` | Non-2xx text/statusText error; missing fields error with JSON; expiry `now + expires_in*1000`. | `read_token_response`; differential exchange/refresh. |
| `exchangeAuthorizationCode` / `refreshAccessToken` | Form bodies and Content-Type; refresh error prefix. | `exchange_authorization_code`, `refresh_access_token`; differential. |
| `startOpenAICodexDeviceAuth` | JSON client id; 404 special message; string interval via `Number(string.trim())`, number interval as-is; finite and non-negative. | `start_device_auth`; `device_code_404_has_the_source_message`, `device_code_interval_uses_javascript_number_parsing`; differential device. |
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

## Settings/models bounded credentialsFromToken correction (2026-10-25)

This lane ports frozen read-token id_token capture, shared JWT decoding, getEmail direct/profile precedence and optional email append in credentialsFromToken. It preserves access/refresh/expires/accountId/email construction order through the shared OAuthCredential key-order metadata and the actual AuthStorage type-first spread. REDs: `/tmp/settings-models-codex-order-red.log`, `/tmp/settings-models-codex-email-red.log`;7 actual-source AuthStorage/Codex refresh cases cover absent/direct/empty/profile/precedence/undecodable/native-unit email and exact file bytes. The rest of this OAuth module remains the AI lane's whole-module audit, not claimed by this bounded slice.

Current lane GREEN: /tmp/settings-models-codex-order-green.log (7 actual-source wrapper cases + native order/email); /tmp/settings-models-oauth-regressions.log (20 Codex/Copilot native cases), /tmp/settings-models-anthropic-regressions.log (6 Anthropic native cases). Targeted warning-mode clippy passes (`/tmp/settings-models-oauth-order-clippy.log`); no warnings on changed lines. Manual changed Rust/TS readability audit found no in-scope violations.
