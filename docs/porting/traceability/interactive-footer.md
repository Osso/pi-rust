# interactive-footer.ts traceability

Source: `packages/coding-agent/src/modes/interactive/components/footer.ts`
Port: `crates/pi-coding-agent/src/modes/interactive/components/footer.rs`

## Exported symbols

| TS export | Rust | Tests |
| --- | --- | --- |
| `formatTokens` | `format_tokens` | `footerHelpers` differential, `footer_formats_tokens_like_the_source` |
| `formatCwdForFooter` | `format_cwd_for_footer` | `footerHelpers` differential, `footer_formats_cwd_relative_to_home` |
| `FooterComponent` | `FooterComponent` | `footer` differential, `footer_renders_override_stats_context_and_model`, `footer_truncates_to_width_and_appends_extension_status` |

## Branches

| Branch | Behavior | Coverage |
| --- | --- | --- |
| `formatTokens` `< 1000` | raw integer | differential `0`, `999` |
| `< 10_000` | `(n/1000).toFixed(1)k` | differential `1000`, `1500`, `9999` |
| `< 1_000_000` | `round(n/1000)k` | differential `10_000`, `12_345`, `999_999` |
| `< 10_000_000` | `(n/1e6).toFixed(1)M` | differential `1_000_000`, `1_500_000`, `9_999_999` |
| otherwise | `round(n/1e6)M` | differential `10_000_000`, `12_345_678` |
| `formatCwdForFooter` inside home | `~` substitution | differential + unit test |
| outside home | original cwd | differential + unit test |
| no home | original cwd | differential `None`, `Some("")` |
| `setSessionOverride` | replaces cwd/model/thinking/usage | `footer` differential index 0 |
| override `session_manager` | name/entries from override manager | `footer` differential index 0 (`Override Session`, 7k/8k/9k/1k usage) |
| override without `session_manager` | live session name/entries | `footer` differential index 1 |
| `clearSessionOverride` | falls back to live session | not covered (gap) |
| pwd line | `cwd` with `~` shortening | full-render differential |
| stats line | executable, tokens, cache hit, cost split | full-render differential |
| context line | percent + context window, `?` when unknown | `footer` differential (25%, unknown) |
| context color | error > 90%, warning > 70% | `footer` differential (75%, 95%) |
| model line | model id, `effort`, provider prefix fallback | full-render differential (`effort high`) |
| extension status lines | `format_extension_status_text_lines` | `footer_truncates_to_width_and_appends_extension_status` |
| auto-compact enabled | ` (auto)` suffix | `footer` differential index 0 |
| narrow width | stats/model truncation | `footer` differential index 1 (width 40) |
| experimental marker | `• xp` suffix | not covered (gap) |

## Lossless checkpoint (2026-10-21)

Cached session names retain `JsString`; `render` calls `encode_node_utf8` only
when constructing the scalar terminal line. Existing footer differential tests
have not run on this migration: the coding-agent library remains compiler-blocked.
Malformed UTF-16 truncation inside scalar TUI components remains an unapproved
display-only parity gap, not a lossless rendering claim.

## Discrepancies

`FooterContextUsage` mirrors the two fields the renderer consumes
(`contextWindow`, `percent`); the unused `tokens` field is not carried.
