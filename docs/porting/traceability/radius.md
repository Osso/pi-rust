# auth/oauth/radius traceability

Pinned source: `packages/ai/src/auth/oauth/radius.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| Constants | Loopback `127.0.0.1:1456/oauth/callback`, 60 s skew, client id/scope, device grant type, login method ids. | Private constants in `radius`. |
| `loadRadiusOAuthDiscovery` | GET `/v1/oauth`; status error with body; `authorizationEndpoint` required. | `load_discovery`; `browser_discovery_requires_the_authorization_endpoint`. |
| `OAuthResponseError` / `readOAuthResponseError` | Status + OAuth error; detail `error: description`, else description or status. | `OAuthResponseError`, `read_oauth_response_error`; differential device/refresh errors. |
| `requestOAuthToken` | Form POST; cancellation; error parsing; typed token; scope extra; 60 s skew. | `request_oauth_token`; differential device + refresh. |
| `startOAuthCallbackServer` | 404/400/200 pages; state check; error/description page; bind failure degrades to manual-only. | `start_callback`; `browser_login_runs_the_callback_and_token_exchange`. |
| `loginWithBrowser` | PKCE + UUID state; authorize query order incl. `handoff=url`; progress/auth URL; cancellation messages; code exchange. | `login_with_browser`; behavioral + differential browser callback. |
| `requestDeviceAuthorization` | Form client/scope; required fields; optional interval. | `request_device_authorization`; differential device. |
| `loginWithDeviceCode` | Device event; poll maps pending/slow_down/expired/denied, rethrows others. | `login_with_device_code`; differential + `device_denials_and_unknown_methods_have_source_messages`. |
| `createRadiusOAuth` | Select prompt/labels; browser discovery; unknown-method error; refresh grant; `toAuth` access token. | `RadiusOAuth` / `create_radius_oauth`; behavioral + differential. |

## Exclusions and discrepancies

- The source constructs `URLSearchParams` for token bodies; Rust uses `url::form_urlencoded` with identical ordering/encoding for the covered fields.
- Callback bind failure is reported as `OAuth callback did not complete.` unless the signal was aborted, matching the source's null-code branch.
- Token responses are typed; missing fields produce a Rust error instead of `undefined` credential fields.
