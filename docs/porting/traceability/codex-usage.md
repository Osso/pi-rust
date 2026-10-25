# `codex-usage` extension traceability

Source: `packages/coding-agent/extensions/codex-usage/src/index.ts`. Rust:
`crates/pi-coding-agent-extensions/src/bundled_extensions/codex_usage/{mod,bridge}.rs`.

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

Focused result at 80ea6e39: native `codex_usage_extension` 13/13 PASS; unchanged differential matches cases 0–12 (including map-empty and completions-empty), then stops at case 13 command-usage because the oracle context lacks `modelRegistry.authStorage` required by frozen lines 552–557. This is oracle-ingress, not evidence of a usage-read production mismatch. `/usage all` remains independently blocked as documented above. Evidence: `codex-completions-green.log`.

Oracle-ingress resolved: the oracle command context now uses frozen `AuthStorage.inMemory` with the same OAuth credential the Rust harness installs (frozen lines 541–560 `authStorage.get/has/getApiKey`) and `isIdle: () => true` (line `sendCodexUsageReport`), replacing the old-pin `getApiKeyForProvider` stub. `differential_codex_usage` now matches every case including 13 `command-usage` onward (`codex-usage-authstorage.log`). `/usage all` remains a gap pending the bridge-request decision.

## Frozen bridge requests (approved native claim API, 2026-10-22)

Supersedes the bridge blocker above. Frozen lines 149–166 route trimmed `all` first, then forward **original** args when the active model provider is `claude-bridge`; literal `claude` is not a special subcommand. `bridge.rs` ports `readClaudeAccountRows`, `formatClaudeAccountRow`, window/identity validation, `sanitizeUsageText`, `dispatchClaudeUsageRequest`, and the all-account table. Native request listeners synchronously assign `EventRequest.response`; command context travels as a native `Arc<dyn Any + Send + Sync>` rather than serialized JSON. This is the approved deviation, not a TypeScript loader or generic RPC API. Unclaimed status and notification strings are unchanged. Handler errors use existing event-bus reporting, not future rejection.

| Branch | Proof |
|---|---|
| Unclaimed accounts; malformed array; rejected future | `differential_claude_usage_bridge`: `all-unclaimed`, `all-malformed`, `all-rejected` |
| Account label/selection, sanitization, null windows, Central summer/winter resets | `all-accounts`, `all-long-control` |
| Invalid identity/window/date/entry; empty array | `all-malformed-entries`, `all-empty` |
| Claimed/unclaimed command, original whitespace/unknown args, rejected handled, all-first routing | `claude-*` fixtures |
| Fan-out, last assignment, listener failures, unsubscribe, native context and raw UTF-16 | `event_bus` request tests |

Oracle uses frozen `AuthStorage.inMemory`, `isIdle: () => true`, and native fixture listeners assigning promises before emit returns. No network or live provider is used.

`differential_claude_bridge_raw_utf16` sends native `JsValue` fixtures to tsx and the Rust listener: unpaired units in account labels, rejected errors, forwarded args and handled rejection; sanitization slices an astral pair at unit 300, preserving its high unit. No serde scalar or replacement projection participates.

### Stored Codex identity and date edges

`ModelRuntime::read_stored_credential` delegates to the existing backing credential store, without the runtime API-key overlay or a parallel metadata copy. `readCodexAccountRow` checks that stored read instead of `list_credentials`, preserving frozen `AuthStorage.has`: runtime-only credentials are not logged-in all-account rows. `readCodexUsage` prefers nonempty stored OAuth `email` over the JWT claim; empty/null metadata retains the claim. `differential_usage_all_codex_credentials` covers all three Codex providers, active selection, stored/empty/null email and runtime-only credentials. Parent authorized this minimal accessor on 2026-10-22.

`all-date-*` covers years 0000/0001/0099 and normalized February overflow/fractional seconds; Central labels use Intl numeric year-of-era (not padded ISO years). Fixture listeners yield once before resolving to prove the claimed future is awaited.

`all-codex-error-redaction` reproduces frozen `readCodexUsage`'s catch boundary: every literal occurrence of the access token is redacted before account-row sanitization and 300-unit clipping. RED showed repeated raw JWTs; Rust now applies the frozen replace-all operation after obtaining the token, preserving pre-token login errors unchanged.

## Whole-module re-pin audit (2026-10-25)

Compared all exports and helpers with frozen `4f9e6f8a4c0065d81b10e7177b28d5082f0bc30d`: token/identity, mapping, formatters, routing/completion, read/reset HTTP, busy reports, all-account bridge and sanitization/date/table helpers. The prior bridge blockers above remain historical and are superseded by the approved claim API section.

Re-pin corrections: HTTP JSON, plan names, rate labels and credit balances stay in existing `JsValue`/`JsString` carriers through mapping and reports; no scalar replacement projection. Missing/nonfinite usage percentages produce absent windows instead of invented 0%. Empty email/account/limit-name/balance formatting follows frozen truthiness; an empty metered feature stays an empty mapped name. Read/reset reports share the all-report busy path: notify now, enqueue with nextTurn. Real HTTP-body seam tests and frozen helper cases reproduce each mismatch before fixing it.

Proof: [lane ledger](../bundled-ext-b-proof.md). Auth fallback boundary remains under audit before closure. Gap: adversarial wrong-type JSON, runtime-only crypto absence, OS faults and numeric extremes use the approved parity exclusions.
