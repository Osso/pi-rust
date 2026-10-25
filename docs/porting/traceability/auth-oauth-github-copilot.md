# auth/oauth/github-copilot traceability

## Common-pin legacy modifyModels filter

Frozen utils/oauth/github-copilot.ts:455–472 preserves unrelated providers, filters Copilot records by availableModelIds when the property exists, retains the full Copilot catalog when absent, and rewrites Copilot baseUrl. Native legacy OAuthProviderInterface::modify_models in oauth_registry.rs follows this path; the modern Models/Provider factory has no filterModels/getAvailable API.

`github_copilot_oauth::legacy_modify_models_filters_present_ids_and_preserves_absent_id_catalog` restores the former filtering coverage at its correct API boundary. Actual frozen-vs-native differential compares complete model records (including raw UTF-16 display name), endpoint mutation, unrelated records, omitted IDs, one ID, empty IDs and a second ID. No network or authentication occurs. Replaces the obsolete stored_oauth_credential_limits_the_provider_catalog test's removed collection-status API, not the filtering requirement.

## Common-pin model deadline

Frozen `packages/ai/src/utils/oauth/github-copilot.ts` at `4f9e6f8a4` creates its own AbortSignal.timeout(5000) for model availability. `fetch_json_with_timeout` now does the same, keeping the deadline through fetch/body rather than substituting a caller signal plus message-only tokio timeout. TimeoutError DOMException/code23 propagates unchanged through AuthError.

`differential_anthropic_timeout::github_model_availability_uses_its_own_timeout_dom_exception` imports the actual frozen refresh helper, observes requested5000ms (accelerating only builtin clock1ms), then compares the native signal and final rejection brand/name/message/code. RED `github-timeout-red.log` → GREEN `github-timeout-green.log`. Existing full-flow evidence below remains historical; broader catalog fallback and caller-signal drift remain outside this bounded proof.

Pinned source: `packages/ai/src/auth/oauth/github-copilot.ts` at
`532f7d12cd19235081bd4d6e425bde185652d86f`.
Rust: `crates/pi-ai/src/auth/oauth/github_copilot.rs`.
Tests: `crates/pi-ai/tests/github_copilot_oauth.rs`,
`crates/pi-ai/tests/differential_github_copilot_oauth.rs`.

| TypeScript | Rust | Proof |
|---|---|---|
| `CLIENT_ID` (`atob`) | `CLIENT_ID` constant | request bodies in `refresh_filters_models_to_the_account_picker_catalog`, differential requests |
| `normalizeDomain` | `normalize_domain` | `normalizes_domains_and_derives_base_urls`, `rejects_invalid_enterprise_domains` |
| `getUrls` | `get_urls` | differential request URLs |
| `getBaseUrlFromToken` | `get_base_url_from_token` | `normalizes_domains_and_derives_base_urls` |
| `getGitHubCopilotBaseUrl` | `get_github_copilot_base_url` | `to_auth_derives_the_proxy_endpoint_from_the_access_token`, differential toAuth |
| `parseAvailableCopilotModelIds` | `parse_available_copilot_model_ids` | `parses_picker_ids_with_the_source_rules`, `refresh_filters_models_to_the_account_picker_catalog`, `falls_back_to_policy_enabled_models_for_individual_accounts_only` |
| `fetchAvailableGitHubCopilotModelIds` (5s timeout) | `fetch_available_model_ids` | differential refresh |
| `fetchJson` status error `"<status> <statusText>: <body>"` | `fetch_json` | differential error case |
| `startDeviceFlow` validation + trusted/normalized `verification_uri` | `start_device_flow` | `device_code_login_reports_details_and_normalizes_the_verification_uri`, `rejects_a_non_http_verification_uri_before_notifying` |
| `pollForGitHubAccessToken` | `poll_for_github_access_token` | `waits_before_polling_and_honors_slow_down_intervals`, `times_out_after_repeated_slow_down_responses` |
| `refreshGitHubCopilotAccessToken` (`expires_at * 1000 - 5min`, `enterpriseUrl`) | `refresh_access_token` | differential refresh |
| `refreshGitHubCopilotToken` (+ `availableModelIds`) | `refresh_token` | differential refresh |
| `enableGitHubCopilotModel` (POST policy, swallow non-abort errors) | `enable_github_copilot_model` | `limits_concurrent_model_policy_updates_during_login`, differential requests |
| `enableAllGitHubCopilotModels` (chunks of 4) | `enable_all_models` (`JoinSet`) | same (max concurrent = 4) |
| `loginGitHubCopilot` prompt/notify/policy/models | `login` | `device_code_login_reports_details_and_normalizes_the_verification_uri`, `limits_concurrent_model_policy_updates_during_login` |
| `copilotEnterpriseDomain` | `copilot_enterprise_domain` | differential refresh (enterprise case) |
| `githubCopilotOAuth.toAuth` | `GitHubCopilotOAuth::to_auth` | `to_auth_derives_the_proxy_endpoint_from_the_access_token`, differential toAuth |

## Branch notes

- Prompt text `"GitHub Enterprise URL/domain (blank for github.com)"` with
  placeholder `company.ghe.com`; aborted signal after the prompt yields
  `Login cancelled`; a non-empty input that does not parse yields
  `Invalid GitHub Enterprise URL/domain`.
- Device-code field validation mirrors the TS `typeof` checks: `interval` must be
  absent or a number; `expires_in` must be a number.
- `verification_uri` is parsed and normalized to `href`; only `http:`/`https:`
  are accepted before the event is emitted.
- Device polling uses the shared device-code helper: wait before the first poll,
  server `slow_down` interval override, and the `slow_down` timeout message.
- `enterpriseUrl` is stored in `OAuthCredential::extra` only when an enterprise
  domain was supplied (`undefined` is dropped by JSON serialization).

## Gaps

- The login flow is not covered differentially (it needs device-code timers);
  `refresh` and `toAuth` are, and the behavioral tests port every
  `github-copilot-oauth.test.ts` case.

## Discrepancies

- None.

Q18 native auth integration and bounded proof status: [lossless auth](lossless-auth.md).

## 2026-10-25 pinned availability repair

Frozen `parseAvailableCopilotModelIds` requires `model_picker_enabled === true`, policy not disabled, and tool calls not false. Removed the old-only individual-account policy fallback and its public flag. Existing actual-source refresh differential reproduced native `["gpt-5.4"]` versus frozen `[]`; unchanged six refresh scenarios and four toAuth scenarios are the proof. Native fixtures now assert the frozen no-picker exclusion for both individual and business accounts. Whole-module status remains partial: login policy scheduling and remaining flow/error audit are not covered by this repair.

2026-10-25 login policy follow-up: frozen Promise.all starts all 28 policy requests together; removed native four-request batches and policy AbortSignal forwarding (source does not pass it). Actual imported login oracle reproduces maximum4 versus maximum28 before repair (`/tmp/ai-copilot-policy-red.log`); uses local fake fetch and a bounded10ms response delay, never a provider. Existing device-code/availability regressions remain required.
