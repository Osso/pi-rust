# auth/oauth/kimi-coding traceability

Pinned source: `packages/ai/src/auth/oauth/kimi-coding.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| Constants | Client id, default host, 900 s device timeout, 5 s default interval, 30 s request timeout, 3 refresh retries. | Private constants in `kimi_coding`. |
| `getOauthHost` | `KIMI_CODE_OAUTH_HOST` then `KIMI_OAUTH_HOST`, else default; trailing slashes removed. | `KimiCodingOAuth::with_runtime` / `with_oauth_host`; `honors_the_oauth_host_override_and_strips_trailing_slashes`. |
| `requestSignal` | 30 s timeout combined with the login signal. | `send_with_timeout`; timeout maps to the abort message. |
| `readJson` | Invalid JSON → `null`. | `send` returns `json: None`; callers use `Value::Null`. |
| `trustedHttpUrl` | http/https only, normalized href. | `trusted_http_url`; differential device. |
| `startDeviceAuthorization` | Form client_id; status error with body; required device fields; interval/expiry defaults. | `start_device_authorization`; differential + behavioral. |
| `parseTokenResponse` | Non-empty access/refresh and positive expiry; missing-fields JSON message. | `parse_token_response`. |
| `pollForToken` | Delayed first poll; 5xx failure; pending/slow_down/expired/denied/other branches. | `poll_for_token`; differential + `device_errors_have_source_messages`. |
| `sleep` / `refreshToken` | Retry delays 1/2/4 s; abort; 401/403/`invalid_grant` unauthorized; retry 429/5xx; final JSON error. | `refresh_token`; `refresh_retries_429_then_succeeds_and_rejects_invalid_grant`; differential refresh. |
| `loginKimiCoding` | Device event uses `verification_uri_complete`. | `login_kimi_coding`; differential device. |
| `kimiCodingOAuth` | Name, subscription, login label, Bearer header `toAuth`. | `KimiCodingOAuth`; behavioral test. |

## Exclusions and discrepancies

- The refresh retry counter is typed as `usize`; the source's attempt loop is `0..=3`, mirrored exactly.
- Cancellation during a retry sleep returns the fetch abort message (`This operation was aborted`) rather than `signal.reason`.
- Token responses are typed; malformed fields produce Rust errors instead of `undefined` credential fields.
