# auth/oauth/xai traceability

Pinned source: `packages/ai/src/auth/oauth/xai.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| Constants | Client id, scope, device/token URLs, 5 min skew, 1 h default lifetime. | Private constants in `xai`. |
| `requiredString` / `positiveNumber` | Non-empty strings; finite positive numbers; field-specific errors. | `required_string`, `positive_number`; `refresh_preserves_an_unrotated_refresh_token_and_defaults_the_lifetime`. |
| `validateVerificationUri` | Parse, require https, return normalized href. | `validate_verification_uri`; `rejects_untrusted_verification_uris`. |
| `postForm` | Accept/Content-Type form POST; cancellation → `Login cancelled`; invalid JSON message. | `post_form`. |
| `requestFailure` | `error`/`error_description` detail. | `request_failure`; denied/expired tests. |
| `parseDeviceCode` | Optional positive interval; optional https complete URI; required device/user/verification/expiry. | `parse_device_code`; differential device + behavioral tests. |
| `credentialsFromTokenResponse` | Access required; refresh preserved when omitted; missing expiry defaults to 3600; expiry with 5 min skew. | `credentials_from_token_response`; refresh tests. |
| `requestDeviceCode` | Form order client_id/scope/referrer. | `request_device_code`; differential body comparison. |
| `pollForTokens` | Delayed first poll; pending/slow_down/denied/expired branches. | `poll_for_tokens`; differential + `denied_and_expired_device_codes_have_source_messages`. |
| `loginXai` | Device event prefers complete URI. | `login_xai`; differential device. |
| `refreshXaiToken` | Refresh grant, preserved token. | `refresh_xai_token`; differential refresh. |
| `xaiOAuth` | Name, subscription, login label, `toAuth` access token. | `XaiOAuth`; behavioral test. |

## Exclusions and discrepancies

- `postForm` parses JSON before checking status, matching the source; a non-JSON error body therefore becomes the invalid-JSON error.
- Differential uses `interval: 1` so polling completes quickly; the timer behavior itself is covered by the shared `device_code` tests.
