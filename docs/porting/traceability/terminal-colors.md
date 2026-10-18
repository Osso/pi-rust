# terminal-colors.ts traceability

Source: `packages/tui/src/terminal-colors.ts` (73 lines). Rust: `crates/pi-tui/src/terminal_colors.rs`.

## Exported symbols

| TypeScript export | Rust item | Tests |
|---|---|---|
| `interface RgbColor` | `struct RgbColor { r, g, b }` | `tests/terminal_colors.rs`, `tests/differential_terminal_colors.rs` |
| `type TerminalColorScheme` | `enum TerminalColorScheme { Dark, Light }` | `parses_color_scheme_reports` |
| `isOsc11BackgroundColorResponse(data)` | `is_osc11_background_color_response(data: &str) -> bool` | `recognizes_the_osc11_response_envelope`, differential |
| `parseOsc11BackgroundColor(data)` | `parse_osc11_background_color(data: &str) -> Option<RgbColor>` | all parse tests, differential |
| `parseTerminalColorSchemeReport(data)` | `parse_terminal_color_scheme_report(data: &str) -> Option<TerminalColorScheme>` | color-scheme tests, differential |

## Branches

| TypeScript branch | Rust branch | Tests |
|---|---|---|
| OSC 11 pattern anchored, `[^\x07\x1b]*`, BEL or ST | `osc11_value` strip/prefix + inner terminator check | `rejects_non_strict_osc11_responses`, differential |
| `value.trim()` | `js_trim` (ECMAScript whitespace set) | `trims_surrounding_whitespace_in_osc11_values` |
| `#` + 6 hex digits | `hex.len() == 6` | `parses_osc11_hex_responses` |
| `#` + 12 hex digits → three 4-digit channels | `hex.len() == 12` | `parses_12_digit_hex_channels` |
| other `#` lengths | `return None` | `rejects_unsupported_hex_lengths` |
| `rgba?:` prefix strip (case-insensitive) | `strip_rgb_prefix` | `accepts_rgba_prefix_and_ignores_extra_channels`, differential `RGB:` |
| split on `/`, require 3 channels | `split('/')` + `next()?` | `rejects_incomplete_or_invalid_rgb_channels` |
| extra channels ignored | `next()` called three times only | `accepts_rgba_prefix_and_ignores_extra_channels` |
| channel regex `^[0-9a-f]+$` | `bytes().all(is_ascii_hexdigit)` | `rejects_incomplete_or_invalid_rgb_channels` |
| `max = 16^len - 1`, `max <= 0` guard | `checked_sub`, `max == 0` guard | `parses_short_and_long_rgb_channels` |
| `Math.round(value / max * 255)` | `(value as f64 / max as f64 * 255.0).round()` | differential exact values |
| repeated `(?:\x1b\[\?997;(1|2)n)+` | `while !rest.is_empty()` loop | `parses_color_scheme_reports` |
| last repeated capture wins | `last` variable | `parses_color_scheme_reports` |
| scheme `2` → light, else dark | `match last` | `parses_color_scheme_reports` |

## Behavioral notes

- `parseInt` on validated hex digits cannot produce `NaN`, so the Rust parser converts with `u8::from_str_radix`/`u64::from_str_radix` after validation.
- The source's 12-digit path calls `hexToRgb(value)` for 6-digit input; `hex_to_rgb` strips `#` itself.
- The TUI integration tests in `packages/tui/test/terminal-colors.test.ts` exercise `TUI.queryTerminalBackgroundColor`; that method belongs to the `tui` module, not this leaf. Only the parsing functions are in scope.

## Gaps / exclusions

- JavaScript callers passing non-string `data` are excluded by the typed Rust API (decision 2026-08-12).

## Discrepancies

None.
