# evals/src/vitest-evals/summary.ts

Source: `packages/evals/src/vitest-evals/summary.ts` (438 lines). Rust:
`crates/pi-evals/src/vitest_evals/summary.rs`.
Tests: `crates/pi-evals/tests/differential_summary.rs`.

## Exported symbols

| TypeScript | Rust |
|---|---|
| `HarnessObservation` | `HarnessObservation` (`base` + `outcome` + `score`) |
| `PairedMetricSummary` | `PairedMetricSummary` |
| `CorrectnessLiftSummary` | `CorrectnessLiftSummary` |
| `HarnessPairComparison` | `HarnessPairComparison` |
| `HarnessComparisonDiagnostic` | `HarnessComparisonDiagnostic` |
| `HarnessEvalSetReport` | `HarnessEvalSetReport` |
| `HarnessComparisonReport` | `HarnessComparisonReport` |
| `summarizeHarnessComparisons` | `summarize_harness_comparisons` |
| `formatHarnessComparisonReport` | `format_harness_comparison_report` / `format_harness_comparison_report_with_style` |

## Differential coverage

`differential_summary::summary_matches_the_typescript_module` sends 15 observation fixtures to the
real TypeScript module and compares both `JSON.stringify(report)` (byte-for-byte, key order included)
and the formatted report in two color modes (`FORCE_COLOR=0` / `FORCE_COLOR=1`). Fixtures:

1. paired correctness + efficiency deltas (TS test)
2. missing observations (TS test)
3. identical inputs in different files (TS test)
4. harness errors (TS test)
5. unscored observations (TS test)
6. multiple candidates (TS test)
7. declared harness without observations (TS test)
8. duplicate + unscorable observations across pairs (TS test)
9. lift/telemetry formatting (TS test)
10. empty report → `""`
11. eval-set `localeCompare` ordering
12. candidate name tie-break ordering
13. partial metric coverage + `toFixed` rounding (1.25/2.5, negative cost)
14. zero deltas, fractional pass rates (`66.7%`), sub-cent costs
15. report JSON round-trip (`report_deserializes_from_its_own_serialization`)

## Branches

- `groupObservations`: insertion-ordered `IndexMap` eval sets/groups, first baseline wins, minimum
  candidate index wins, `JSON.stringify([file, testName, groupKey])` via `js_json::stringify`.
- `mean`, `preciseDifference`, `summarizeCorrectness`, `summarizeMetric`: ported; `totalPairs` is the
  eval-set group count, eligibility requires both sides `scored` and finite metrics.
- `collectDiagnostics`: reason precedence (missing → duplicate → harness-error → missing-score →
  unscorable) and sort order (evalSet, file, groupKey, repetition, harness).
- Formatting: `padStart(9)`, `toFixed(1/4)`, signed deltas, coverage suffix only when partially
  eligible, `colorDelta` green/red/gray, `unavailable` yellow.

## Gaps

- `Number((x - y).toPrecision(15))` is implemented as exact 15-significant-digit formatting
  (`format!("{difference:.14e}")`) followed by parsing. ECMAScript's ties-away-from-zero differs from
  Rust's ties-to-even only on exact decimal ties, which the differential fixtures do not hit.
- `styleText` color support is resolved by `StyleText::detect` (Node's `process.stderr.hasColors()`
  rules); the explicit `StyleText` argument is a test seam absent from the source.
