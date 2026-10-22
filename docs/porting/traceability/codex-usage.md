# `codex-usage` extension traceability

Source: `packages/coding-agent/extensions/codex-usage/src/index.ts`. Rust:
`crates/pi-coding-agent/src/bundled_extensions/codex_usage/mod.rs`.

| TypeScript symbol / branch | Rust item | Proof |
|---|---|---|
| `resolveCodexAccountId` / `resolveCodexAccountInfo` | `resolve_codex_account_id`, `resolve_codex_account_info` | differential `token-account`, `token-profile`, `token-missing-account`; behavioral `extracts_account_and_email_claims` |
| `resolveCodexDisplayEmail` (stored email wins) | `resolve_codex_display_email` | differential `display-stored`, `display-claim` |
| `decodeCodexTokenPayload` (`parts.length !== 3` error) | `decode_codex_token_payload` | behavioral invalid-token assertion |
| `mapCodexUsagePayload` (codex + additional limits, credits, reset credits) | `map_codex_usage_payload`, `map_codex_rate_limit`, `map_window`, `map_credits` | differential `map-base`, `map-empty`, `map-additional` |
| `formatCodexUsage` / `formatLimit` / `formatLimitDetails` / `formatWindow` / `formatPercent` / `formatResetTime` / `formatCredits` | `format_codex_usage` and helpers | differential `format-identity`, `format-anonymous`; behavioral exact-text test |
| `getArgumentCompletions` (`reset` prefix, null when none) | `argument_completions` | differential `completions-*`; behavioral test |
| `usage` command (`reset`, invalid args, read usage) | `handle_usage_command` | differential `command-*` |
| `readCodexUsage` (provider token, account info, GET, JSON) | `read_codex_usage` | differential `command-usage`, `command-failed`, `command-empty-body` |
| `resolveCodexProviderId` (model api `openai-codex-responses` else default) | `resolve_codex_provider_id` | differential `command-gc-provider` |
| `readCodexAccessToken` (login error text) | `read_codex_access_token` | differential `command-missing-login`; behavioral test |
| `createCodexHeaders` | `create_codex_headers` | differential request headers |
| `readJsonResponse` (status error, empty body `{}`) | `read_json_response` | differential `command-failed`, `command-empty-body` |
| `resetCodexUsageAndSendResult` (no credits, confirm, consume, re-read) | `reset_codex_usage_and_send_result` | differential `reset-declined`, `reset-no-credit`, `reset-success` |
| `consumeCodexUsageResetCredit` (UUID request id, POST) | `consume_codex_usage_reset_credit` | differential `reset-success` request |
| `formatResetResponse` | `format_reset_response` | behavioral `formats_reset_responses` |
| `sendCodexUsageMessage` (`customType: "codex-usage"`, `display: true`) | `send_codex_usage_message` | differential messages |

## Shared helpers

- `pi_ai::request::read_body_text` implements `Response#text` (UTF-8 lossy).
- `pi_js_compat::date::utc_date_fields` implements `getUTC*` for `formatResetTime`.
- `pi_js_compat::number::js_number_value` is used by the loop extension; this
  module serializes numeric fields through the existing `number::serialize`
  helpers.

## Discrepancies / gaps

- `crypto.randomUUID()` is `pi_ai::utils::uuid::random_uuid()`; both are random
  UUIDv4 values and are normalized in the differential.
- HTTP goes through `pi_ai::request::Fetch`; the extension factory accepts an
  injected fetch, defaulting to `pi_ai::http::default_fetch`.

## Q18 missing reset-credit DRIFT

Observed RED at c9cf6ea1: `differential_codex_usage` case `map-empty` returned Rust `resetCreditsAvailable: 0`, frozen TS `null` (`codex-fixtures-green.log`). Frozen `extensions/codex-usage/src/index.ts:73,121` explicitly retains `number | null`; line 131 defaults only display and line 498 defaults reset eligibility. Parent approved correction on 2026-10-22. Rust now retains `Option<f64>` and applies zero only at those consuming branches; the existing map/format/reset differential remains the behavioral proof. Focused GREEN pending.

Q18 follow-up: null-credit fix 668639e0 passes differential cases 0–10, then case 11 `completions-empty` is RED (Rust reset only vs frozen all+reset). Frozen lines 142–146 and 161–164 now drive completions/help text. `/usage all` remains a re-pin gap: frozen lines 208–297 require synchronously mutable request objects containing asynchronous accounts/handled fields; Rust `core/extensions/types.rs:2989–3001` accepts immutable JSON and `core/event_bus.rs:7,17–19` supplies only `Fn(&Value)`. Parent owns that contract change; no bus deviation implemented.
