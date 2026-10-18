# auth/oauth/oauth-page traceability

Pinned source: `packages/ai/src/auth/oauth/oauth-page.ts` at commit `532f7d12cd19235081bd4d6e425bde185652d86f`.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `escapeHtml` | Replaces `&`, `<`, `>`, `"`, `'` in that order. | `oauth_page::escape_html`; `renders_the_success_page_with_escaped_message`, `escapes_ampersands_before_other_entities`. |
| `renderPage` | Fixed doctype/head/style/logo template; optional details block only when `details` is truthy (empty string omitted); all interpolated text escaped. | `render_page` with the same template constants; `renders_the_error_page_with_optional_escaped_details` includes the empty-string case. |
| `oauthSuccessHtml(message)` | Title/heading `Authentication successful`, no details. | `oauth_success_html`; `renders_the_success_page_with_escaped_message`. |
| `oauthErrorHtml(message, details?)` | Title/heading `Authentication failed`, optional escaped details. | `oauth_error_html`; `renders_the_error_page_with_optional_escaped_details`. |
| Differential oracle | 12 cases: empty/special/unicode/newline messages, missing/empty/special details. | `tests/differential_oauth.rs` compares full HTML strings byte-for-byte against the pinned module. |

## Exclusions

- None. Output is deterministic and compared exactly.
