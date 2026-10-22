# agent-core truncate — common pin

Frozen `packages/agent-core/src/harness/utils/truncate.ts:1–344`, 4f9e6f8a4. Rust `harness/utils/truncate.rs`.

| Frozen symbol / branches | Proof |
|---|---|
| constants/options/result types (1–53) | `truncate::constants_and_default_options_match_source`, full result comparisons |
| UTF-8 length and replacement helpers (55–110) | `lossless_core_utils` 20-case raw-unit differential matrix, retained `differential_truncate` tail corpus and Buffer-unavailable oracle |
| `formatSize` (115–123) | eight integer threshold differential cases plus 11 full-number-domain cases: fractional/negative/negative-zero, NaN, infinities, exponent notation and rounding thresholds |
| `truncateHead` (133–210) | 18 focused truncation tests and retained differential fixtures; terminal empty split line preserved; empty input is one split line |
| `truncateTail` (220–284), byte suffix helper (291–330) | differential byte-tail corpus, partial/whole lines, zero limits and 20 native matrix cases; raw unpaired units preserved when untouched, Node UTF-8 replacement when encoded for a truncated suffix |
| `truncateLine` (336–344) | raw-unit slice differential, including a cut inside a valid surrogate pair; no former scalar-count deviation |

RED: repointed `differential_truncate` detected head trailing-empty-line mismatch (Rust 2 vs TS 3). GREEN: retained differential plus 18 focused tests; new raw-unit differential matrix. Logs `/tmp/pi-rust-repin-delta-agent-core/{utils-green,utility-loader-green,utils-native-green}.log`.

Numeric RED→GREEN: `utils-number-red.log` exposed usize truncation of 1.5 and incorrect byte/line limit cause; `negative-fraction-red.log` exposed ToIntegerOrInfinity(-0.5) before slicing. `TruncationOptions`, emitted numeric limits, `format_size` and `truncate_line` limits now use f64, preserving the complete frozen number domain. New numeric oracle compares the unchanged frozen functions including NaN/Infinity JSON.stringify behavior; real result serialization replaces duplicate hand-built fixture output. `numeric-core-final.log`: native UTF-16/numeric oracle and retained utility oracle GREEN. `numeric-coding-final.log`: 7 focused caller tests + full coding-agent truncation differential GREEN; coding-agent callers now convert integer counts at the f64 API boundary. No assertion is loosened or filtered.
