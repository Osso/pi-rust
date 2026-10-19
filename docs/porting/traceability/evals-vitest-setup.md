# evals/src/vitest-evals/setup.ts

Source: `packages/evals/src/vitest-evals/setup.ts` (8 lines). Rust:
`crates/pi-evals/src/vitest_evals/setup.rs`.
Tests: `crates/pi-evals/tests/artifacts.rs::setup_records_the_harness_session_snapshot`.

## Behavior

The TypeScript `afterEach` hook reads `task.meta.harness?.run` and calls
`recordEvalSessionArtifact(task, run)` when a run exists.

Rust exposes `record_eval_session_artifact_after_run(&mut EvalTestCase)`:
- no harness → no-op;
- harness present → `record_eval_session_artifact(&mut test.artifacts, &harness.run)`;
- no session snapshot → no-op; invalid metadata → `TypeError`.

## Gaps

- The `afterEach` registration itself is Vitest runner wiring; the Rust runner calls the function
  after each eval case.

## Discrepancies

- None.
