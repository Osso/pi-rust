# coding-agent footer-status traceability

Pinned source: `packages/coding-agent/src/utils/footer-status.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Differential proof: request `26004` executes the real pinned module across 6 sanitize and 7 line-formatting cases. Focused tests: `crates/pi-coding-agent/tests/footer_status.rs`.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `footer-status.ts:3-8` | Replace CR/LF/TAB with spaces, collapse runs of ASCII spaces, trim | `utils::footer_status::sanitize_footer_status_text`; focused `sanitizes_control_whitespace_and_collapses_spaces` |
| `footer-status.ts:10-12` | Sort status keys with `localeCompare` | `BTreeMap` key order; focused `joins_non_goal_statuses_in_key_order_before_the_goal`; differential ASCII keys |
| `footer-status.ts:12` | Extract the exact `goal` key | `format_extension_status_text_lines`; focused goal cases |
| `footer-status.ts:13-17` | Sanitize non-goal texts, join with one space, sanitize the goal | `format_extension_status_text_lines`; focused and differential cases |
| `footer-status.ts:19` | Return other-status line first, then goal; drop absent lines, but keep an empty line when a status key exists | Focused empty/only-goal/only-other/empty-text cases; differential case 12 |

## Discrepancies

- `localeCompare` uses ICU collation; the Rust port uses ordinary Unicode scalar ordering. ASCII status keys match; non-ASCII collation order is an explicit typed-boundary deviation.
- Rust `trim()` and JavaScript `trim()` both remove Unicode whitespace; the source's `.replace(/ +/g, " ")` collapses ASCII spaces only, which the port preserves.

## Proof inventory

- Focused cases: 7.
- Differential request: `26004`, 13 cases.

## Order audit

`formatExtensionStatusTextLines` sorts non-goal keys with `pi_js_compat::locale_compare`, matching
`localeCompare`; the `BTreeMap` input order is therefore not observable. Differential `26004` covers the
resulting lines.

Native status text uses `JsString`, with native IndexMap input and ICU UTF16 locale comparison. Control whitespace flattening and repeated-space collapse retain every other native unit; final trim follows JS, not Rust Unicode whitespace. `raw_footer_statuses` compares high/low/pair formatting, goal-line separation and sort output to frozen helpers. The pre-migration sanitizer removed U+0085 although JS trim preserves it (behavioral RED), now corrected. Direct renderer consumers must remain native; no encoding adapter is provided.
