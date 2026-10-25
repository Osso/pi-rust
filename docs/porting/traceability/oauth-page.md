# auth/oauth/oauth-page traceability

Pinned source: `packages/ai/src/utils/oauth/oauth-page.ts` at `4f9e6f8a4` (109 lines). [Contract](../../specs/oauth-page.md).

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `escapeHtml` | Replaces `&`, `<`, `>`, `"`, `'` in that order. | `oauth_page::escape_html`; `renders_the_success_page_with_escaped_message`, `escapes_ampersands_before_other_entities`. |
| `renderPage` | Fixed doctype/head/style/logo template; optional details block only when `details` is truthy (empty string omitted); all interpolated text escaped. | `render_page` with one-pass interpolation of the same template; literal marker-text cases; `renders_the_error_page_with_optional_escaped_details` includes the empty-string case. |
| `oauthSuccessHtml(message)` | Title/heading `Authentication successful`, no details. | `oauth_success_html`; `renders_the_success_page_with_escaped_message`. |
| `oauthErrorHtml(message, details?)` | Title/heading `Authentication failed`, optional escaped details. | `oauth_error_html`; `renders_the_error_page_with_optional_escaped_details`. |
| Differential oracle | 14 cases: empty/special/unicode/newline messages, missing/empty/special details. | `tests/differential_oauth.rs` compares full HTML strings byte-for-byte against the pinned module. |

## Current slice

Observed actual-source RED: Native sequential placeholder replacements rewrite literal message/details text; frozen uses one-pass interpolation. The renderer now uses one-pass formatting and retains the exact ordered `escapeHtml` replacements. Missing details are sent as an omitted field by the differential driver, matching the typed optional argument. Atc2b1eea85, current native3 and actual-source differential1 (14 complete HTML outputs) pass. Required two-package all-target check passes; scoped Clippy0 errors/11 existing warnings/0 changed-line warnings. Logs: `/tmp/ai-oauth-page-{source-green,native-green,check,clippy}*`. Manual changed-line readability is clean; renderer length is the frozen HTML literal, not added control flow.

## Exclusions

Lone UTF-16 units beyond the existing scalar input carrier are a recorded gap under the 2026-10-25 parity bar. Marker-like text is not a gap (explicit parent decision).
