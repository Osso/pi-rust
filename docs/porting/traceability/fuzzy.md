# fuzzy.ts traceability

Source: `packages/tui/src/fuzzy.ts` (161 lines). Rust: `crates/pi-tui/src/fuzzy.rs`.

## Exported symbols

| TypeScript export | Rust item | Tests |
|---|---|---|
| `interface FuzzyMatch` | `struct FuzzyMatch { matches, score }` | `tests/fuzzy.rs` (all) |
| `fuzzyMatch(query, text)` | `fuzzy_match(query: &str, text: &str) -> FuzzyMatch` | `tests/fuzzy.rs` cases; `tests/differential_fuzzy.rs` |
| `fuzzyFilter<T>(items, query, getText)` | `fuzzy_filter<T: Clone>(items: &[T], query: &str, get_text: impl Fn(&T) -> Vec<String>) -> Vec<T>` | `tests/fuzzy.rs` filter cases; `tests/differential_fuzzy.rs` |

## Branches

| TypeScript branch | Rust branch | Tests |
|---|---|---|
| empty normalized query → match, score 0 | `query.is_empty()` | `empty_query_matches_everything_with_score_zero` |
| query longer than text → no match | `query.len() > text.len()` | `query_longer_than_text_does_not_match` |
| character scan, order enforced | `for unit in text` loop | `characters_must_appear_in_order` |
| consecutive-match reward (`lastMatchIndex === i - 1`) | `is_consecutive` with `last_match_index = -1` seed | `exact_match_scores_negative`, differential |
| gap penalty | `last_match_index >= 0` branch | differential |
| word-boundary reward (`/[\s\-_./:]/`) | `is_word_boundary_unit` (ECMAScript `\s` set) | `word_boundary_matches_score_better`, differential `x\u00a0y` |
| later-match penalty (`i * 0.1`) | `score += i as f64 * 0.1` | differential exact score bits |
| exact-match bonus (`-100`) | `normalized_query == text_lower` | `exact_match_scores_negative` |
| alpha/digit swap fallback | `swapped_alphanumeric` | `swapped_alphanumeric_tokens_match`, differential |
| swapped-match `+5` | `swapped_match.score + 5.0` | differential |
| `fuzzyFilter` empty trimmed query | `js_trim` + `is_empty` | `filtering_returns_items_unchanged_for_empty_query` |
| token split on `/[\s/]+/` | `split` on ECMAScript whitespace or `/` | `filtering_splits_whitespace_and_slash_separated_tokens` |
| per-candidate best score | `best_token_match` | `filtering_supports_multiple_candidates_without_spanning_them` |
| stable ascending sort | `sort_by(partial_cmp)` | `filtering_sorts_by_match_quality` |

## Behavioral notes

- Scoring iterates UTF-16 code units (`encode_utf16`), matching JavaScript string indexing for astral characters. Differential case `😀` / `a😀b` asserts exact score bits.
- The source seeds `lastMatchIndex = -1`, so the first matched character at index 0 satisfies `lastMatchIndex === i - 1` and receives the consecutive bonus. This is preserved (not a bug fix).
- `String.prototype.toLowerCase()` is reproduced with Rust `str::to_lowercase`; differential cases cover `Ä`/`ä`, `İ`, `ß`/`SS`.
- `String.prototype.trim()` is reproduced with the ECMAScript whitespace set (`js_trim`), including U+FEFF and U+00A0 but excluding U+0085.

## Gaps / exclusions

- `getText` returning `string | string[]` is typed as `Fn(&T) -> Vec<String>`; callers returning a single string wrap it in a one-element vector. No runtime behavior is excluded.
- JavaScript callers passing non-string `query`/`text` are excluded by the typed Rust API (decision 2026-08-12).

## Discrepancies

None found. Exact score bits are compared through the differential oracle (`scoreBits`), avoiding JSON decimal re-parsing.
