# evals/src/vitest-evals/harness-table.ts

Source: `packages/evals/src/vitest-evals/harness-table.ts` (193 lines). Rust:
`crates/pi-evals/src/vitest_evals/harness_table.rs`.
Tests: `crates/pi-evals/tests/differential_harness_table.rs`.

## Exported symbols

| TypeScript | Rust | Tests |
|---|---|---|
| `EVAL_HARNESS_ITERATION_ARTIFACT` | same constant | plan/artifact tests |
| `EvalHarnessIterationArtifact` | `EvalHarnessIterationArtifact` (created order) and `ParsedEvalHarnessIterationArtifact` (parse literal order) | parse differential |
| `EvalHarnessTableRow` | `EvalHarnessTableRow<I>` | plan differential |
| `EvalHarnessTablePairOptions` / `CandidatesOptions` / `Options` | same, `EvalHarnessTableOptions<I>` union | plan/validation differential |
| `parseEvalHarnessIterationArtifact` | `parse_eval_harness_iteration_artifact` | 10 fixtures: valid, fractional repetition, wrong schema, wrong types, non-array candidates, non-string candidate, string repetition, array, null, undefined |
| `deriveEvalGroupKey` | `derive_eval_group_key` | 20 fixtures: trimmed `id`, canonical key order, nested values, array-index keys, UTF-16 key order (BMP/non-BMP), string/number/bool/null inputs, `1e21`, `-0`, numeric/blank `id` |
| `evalHarnessTable` | `eval_harness_table` | plan differential + validation differential |
| `withIterationArtifact` error path (`getHarnessRunFromError` → `attachHarnessRunToError(error, attachIterationArtifact(partialRun))`; other errors rethrown untouched) | `with_iteration_artifact` + `attach_iteration_artifact`; `PiEvalError::{harness_run, with_harness_run, take_harness_run}` carries the `vitestEvalsRun` property | `eval_harness_table_attaches_the_iteration_artifact_to_a_partial_run_like_typescript`: oracle fixtures `partial` (run artifacts incl. a stale iteration key), `partial_without_artifacts`, `plain` (no run); error class/message and the attached run compared byte-for-byte |

## Differential coverage

- `derive_eval_group_key_matches_the_typescript_module`: compares the group key string.
- `parse_eval_harness_iteration_artifact_matches_the_typescript_module`: compares
  `JSON.stringify(parsed)` byte-for-byte (the parsed literal order differs from the created order in
  the source, so Rust uses two structs).
- `eval_harness_table_plan_matches_the_typescript_module`: runs the wrapped fake harness for every
  row and compares row names/repetitions plus the merged run artifacts byte-for-byte.
- `eval_harness_table_attaches_the_iteration_artifact_to_a_partial_run_like_typescript`: the oracle
  harnesses throw via the real `attachHarnessRunToError` (resolved from the pinned `vitest-evals`);
  the attached run's artifacts are `{...context.artifacts, ...run.artifacts, iteration}` in TS key order.
- `eval_harness_table_rejects_invalid_options_like_typescript`: empty eval set, no candidates,
  duplicate names, zero repetitions; error class and message compared.

## Gaps

- `canonicalizeJson` error branches for `Date`, sparse arrays, circular references, and non-JSON
  `typeof` values are unrepresentable in the typed Rust input (`serde_json::Value` / `Serialize`);
  the finite-number check remains for JSON numbers.
- `HarnessContext` is passed to the inner harness by value, so an inner harness calling
  `setArtifact` on its copy is not visible to the wrapper's `...context.artifacts` spread (TS shares
  the context object). The real Pi harness returns its artifacts in the run, so no ported harness hits
  this; a shared context is a `HarnessContext` API change outside this module.
- `HarnessRun` always serializes `artifacts` at its struct position; in TS a run object lacking an
  `artifacts` key gains it last. Reporters read by field name, so only raw serialized key order of such
  runs differs.
- `repetitions` is `u32` (positive integer domain) instead of an arbitrary JS number; non-integer and
  unsafe-integer values are typed out.

## Discrepancies

- None.
