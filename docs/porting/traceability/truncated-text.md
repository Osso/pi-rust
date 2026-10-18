# components/truncated-text.ts traceability

Source: `packages/tui/src/components/truncated-text.ts` (65 lines). Rust:
`crates/pi-tui/src/components/truncated_text.rs`.

Tests: `crates/pi-tui/tests/truncated_text.rs` (all nine listed TS tests
ported), `crates/pi-tui/tests/differential_components.rs::spacer_dynamic_border_and_truncated_text_match_typescript`.

## Exported symbols

| TypeScript export | Rust item | Tests |
|---|---|---|
| `class TruncatedText` | `TruncatedText` | all |
| `constructor(text, paddingX = 0, paddingY = 0)` | `TruncatedText::new(text, padding_x, padding_y)` | all |
| `invalidate` | `Component::invalidate` (no-op) | source has no cached state |
| `render(width)` | `Component::render` | all |

## Branches

| TypeScript branch | Rust branch | Tests |
|---|---|---|
| vertical padding before/after content | `render` loops | `pads_output_with_vertical_padding_lines_to_width` |
| available width `max(1, width - paddingX*2)` | `saturating_sub(...).max(1)` | differential width cases |
| first line only (stop at `\n`) | `split('\n').next()` | newline tests |
| truncate with default `"..."` ellipsis | `truncate_to_width` | long-text tests |
| ANSI-preserving truncation and reset before ellipsis | `truncate_to_width` | styled tests |
| pad line to exactly width | `visible_width` + spaces | all |

## Behavioral notes

- Empty text still produces one padded line, matching the source.
- Differential cases cover padding, long text, ANSI, newlines, and empty text.

## Gaps / exclusions

- JavaScript callers passing negative padding are excluded by the typed API.

## Discrepancies

None found.
