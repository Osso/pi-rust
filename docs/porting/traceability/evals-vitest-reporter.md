# evals/src/vitest-evals/reporter.ts

Source: `packages/evals/src/vitest-evals/reporter.ts` (111 lines). Rust:
`crates/pi-evals/src/vitest_evals/reporter.rs`.
Tests: `crates/pi-evals/tests/reporter.rs`.

## Exported symbols

| TypeScript | Rust |
|---|---|
| `EvalHarnessReporter` (default export) | `EvalHarnessReporter` |
| `onTestCaseResult` | `on_test_case_result` |
| `onTestRunEnd` | `on_test_run_end` |
| (module-private) `appendHarnessRunReport` | `append_harness_run_report` |
| (module-private) `collectHarnessObservations` | `collect_harness_observations` |

## Behavioral coverage

`reporter_writes_runs_and_the_comparison_report` builds a baseline/candidate module pair, records the
session artifacts through the setup hook, and asserts:

- `runs.jsonl` has one byte-exact line per test (field order, skipped `reasoningTokens`/`retries`,
  `timings`, persisted `artifacts` references, and the `metadata` iteration artifact);
- the persisted session body;
- `collect_harness_observations` order and harness names;
- the comparison report text (lift, tokens delta, coverage);
- the interrupted-run message;
- no `PI_EVAL_ARTIFACT_DIR` → no append.

`reporter_includes_run_errors_and_marks_harness_errors` asserts the conditional `errors` field and
the `errored` observation outcome.

## Branches

- `readFiniteNumber` for `avgScore` and `estimatedCostUsd`.
- `runId` fallback to `randomUUID()` (`pi_ai::utils::uuid::random_uuid`).
- Metadata = artifacts minus `runId`/`piSessionJsonl`, omitted when empty.
- Observation outcome precedence: run errors → `errored`; finite score → `scored`; else test state
  (`passed` → `unscored`, `failed` → `errored`, `skipped`/`pending` preserved).
- `runs.jsonl` opened with append/create and flushed explicitly (Tokio `File` buffers writes until
  flush, unlike Node `appendFile`).
- `interrupted` → `\nEval comparisons unavailable: test run interrupted.`; otherwise the formatted
  report prefixed with `\n`, or `null` when empty.

## Gaps

- Vitest reporter registration and `vitest.logger.log` are replaced by explicit methods; the Rust
  `on_test_run_end` returns the text the source logs.
- `test.meta()`/`test.result()`/`module.children.allTests()` are modeled as `EvalTestCase` fields and
  a pre-flattened `TestModule.tests` list.
- `SerializedError[]` passed to `onTestRunEnd` is unused in the source and not modeled.

## Discrepancies

- None.
