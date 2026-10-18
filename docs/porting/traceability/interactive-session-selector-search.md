# session-selector-search.ts

Source: `coding-agent/src/modes/interactive/components/session-selector-search.ts`
Rust: `crates/pi-coding-agent/src/modes/interactive/components/session_selector_search.rs`

| Symbol / branch | Rust item | Proof |
|---|---|---|
| `SortMode` | `SortMode` | differential + `session_selector_search.rs` |
| `NameFilter` | `NameFilter` | name-filter tests |
| `ParsedSearchQuery` | `ParsedSearchQuery` (`mode`, `tokens`, `regex`, `error`) | differential `parse` |
| `MatchResult` | `MatchResult` | differential `match` |
| `normalizeWhitespaceLower` | `normalize_whitespace_lower` (JS whitespace set, per-char lowercase) | phrase test with `\n\n   ` |
| `getSessionSearchText` | `get_session_search_text` | differential `match` |
| `hasSessionName` | `has_session_name` (`js_trim`) | whitespace-name test |
| `hasSessionMessages` | `has_session_messages` | empty-session test |
| `matchesNameFilter` | `matches_name_filter` | name-filter tests |
| `parseSearchQuery` empty | early return | differential `""` |
| `re:` empty pattern → `Empty regex` | error branch | differential `re:` |
| `re:` valid → case-insensitive regex | `RegexBuilder::case_insensitive` | regex test + differential |
| `re:` invalid → error | error branch | `re:(` test + differential `errorPresent` |
| quoted tokens / fuzzy tokens | token loop | differential `parse` |
| unclosed quote → whitespace fallback | `had_unclosed_quote` branch | differential `unclosed "quote` |
| `isLiteralTokenQueryMatch` | `is_literal_token_query_match` | `foo bar` ranking tests |
| phrase `indexOf` score `idx * 0.1` | `utf16_index` + score | relevance score test |
| fuzzy token score | `pi_tui::fuzzy_match` | differential scores |
| `matchSession` regex / empty tokens | `match_session` branches | differential |
| recent: literal group then fuzzy group, input order | `filter_and_sort_sessions` recent branch | recent ranking tests |
| relevance: literal, score, modified desc (stable) | `sort_by` comparator | relevance tests |
| error → `[]` | `parsed.error.is_some()` | invalid-regex test |
| name filter before search | `name_filtered` first | name-filter tests |

Discrepancy: the stored `error` string for an invalid regex is the Rust `regex` crate message, not V8's `Invalid regular expression: ...`. The observable behavior (query treated as non-matching, `[]` result) is identical, and the differential compares `errorPresent` rather than the message text. Regex dialect differences between V8 and `regex` (backreferences/lookaround) are not reproduced; the source only constructs plain `RegExp(pattern, "i")`.
