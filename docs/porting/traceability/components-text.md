# components/text.ts traceability

Source: `packages/tui/src/components/text.ts` (106 lines). Rust:
`crates/pi-tui/src/components/text.rs`.

Tests: `crates/pi-tui/tests/text.rs`,
`crates/pi-tui/tests/differential_components.rs::text_render_matches_typescript`.

## Exported symbols

| TypeScript export | Rust item | Tests |
|---|---|---|
| `class Text` | `struct Text` | `tests/text.rs` (all), differential |
| `constructor(text = "", paddingX = 1, paddingY = 1, customBgFn?)` | `Text::default()` (`""`, 1, 1), `Text::new(text, padding_x, padding_y)`, `Text::with_background(...)` | padding/background tests, differential |
| `setText` | `Text::set_text` | `caches_render_until_text_or_width_changes` |
| `setCustomBgFn` | `Text::set_custom_bg_fn` | `background_change_invalidates_cache` |
| `invalidate` | `Component::invalidate` | cache tests |
| `render(width)` | `Component::render` | all |

## Branches

| TypeScript branch | Rust branch | Tests |
|---|---|---|
| cache hit on text + width | `cached_lines/cached_width/cached_text` check | `caches_render_until_text_or_width_changes` |
| empty or whitespace-only text returns `[]` | `text.trim().is_empty()` | `renders_nothing_for_empty_or_whitespace_text` |
| tabs replaced by three spaces | `replace('\t', "   ")` | `wraps_words_and_expands_tabs`, differential |
| content width `max(1, width - paddingX*2)` | `saturating_sub(...).max(1)` | differential `界界界` width 3 |
| word wrap via `wrapTextWithAnsi` | `wrap_text_with_ansi` | `wraps_words_and_expands_tabs`, differential |
| margins + background padding vs plain space padding | `pad_to_width` | padding/background tests, differential |
| top/bottom padding lines (background-aware) | `build_lines` empty lines | `applies_background_to_content_and_padding_lines` |
| `result.length > 0 ? result : [""]` | final fallback in `render` | not reachable with non-empty text; documented below |

## Behavioral notes

- Differential cases cover padding 0/1/2, background on/off, tabs, CJK, and
  embedded newlines; rendered lines are compared byte-for-byte.
- `Text::default()` matches the source defaults; `Text::new` requires explicit
  padding values because Rust has no optional arguments.

## Gaps / exclusions

- The `result.length > 0 ? result : [""]` fallback is unreachable: non-empty
  text always produces at least one content line. Kept for source parity.
- JavaScript callers passing non-string text or negative/fractional padding are
  excluded by the typed Rust API (decision 2026-08-12).

## Discrepancies

None found.
