# overflow traceability

Source: `packages/ai/src/utils/overflow.ts`.
Rust: `crates/pi-ai/src/utils/overflow.rs`.
Tests: `crates/pi-ai/tests/overflow.rs` (13 behavioral), `crates/pi-ai/tests/differential_overflow.rs` (40 checks + 35-sample pattern matrix vs pinned TS).

## Representation
- Patterns are compiled once in `OnceLock<Vec<Regex>>` with a `(?i)` prefix, replacing JS `new RegExp(..., "i")`.
- `contextWindow?: number` → `Option<f64>`; JS truthiness is mirrored: `Some(0.0)`/`None`/NaN behave as absent.

## Symbols → proof
| TS | Rust | Test |
|---|---|---|
| `isContextOverflow` error branch | `is_context_overflow` | 13 behavioral tests + differential checks |
| `NON_OVERFLOW_PATTERNS` precedence | `non_overflow_regexes` | `does_not_treat_bedrock_throttling_as_overflow`, `does_not_treat_rate_limits_as_overflow` |
| silent overflow (`stop`, `input + cacheRead > window`) | `is_context_overflow` | differential checks with `contextWindow` 0/null/32768 |
| length-stop overflow (`length`, `output == 0`, `input + cacheRead >= 0.99 * window`) | `is_context_overflow` | `detects_xiaomi_style_length_stop_with_zero_output_and_filled_context`, `normal_length_stops_are_not_context_overflow` |
| `isRecoverableLength` | `is_recoverable_length` | `recoverable_length_stops_below_the_desired_output_limit` |
| `getOverflowPatterns` | `get_overflow_patterns` | `exposes_the_overflow_patterns_for_testing`, differential pattern matrix |

## Branch notes
- All 26 patterns are ported verbatim, including the anchored Cerebras `^4(00|13)…(no body)` pattern.
- Rust regex `.` matches any character except `\n`, like JS without `s`.
- `\d` in the `regex` crate is Unicode-aware; JS `\d` is ASCII. Provider error strings use ASCII digits, so the differential matrix covers all documented wordings.

## Gaps
- None.

## Discrepancies
- None.
