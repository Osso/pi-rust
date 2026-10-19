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

## Differential coverage

- `derive_eval_group_key_matches_the_typescript_module`: compares the group key string.
- `parse_eval_harness_iteration_artifact_matches_the_typescript_module`: compares
  `JSON.stringify(parsed)` byte-for-byte (the parsed literal order differs from the created order in
  the source, so Rust uses two structs).
- `eval_harness_table_plan_matches_the_typescript_module`: runs the wrapped fake harness for every
  row and compares row names/repetitions plus the merged run artifacts byte-for-byte.
- `eval_harness_table_rejects_invalid_options_like_typescript`: empty eval set, no candidates,
  duplicate names, zero repetitions; error class and message compared.

## Gaps

- `canonicalizeJson` error branches for `Date`, sparse arrays, circular references, and non-JSON
  `typeof` values are unrepresentable in the typed Rust input (`serde_json::Value` / `Serialize`);
  the finite-number check remains for JSON numbers.
- `withIterationArtifact` attaches a partial run when `getHarnessRunFromError` finds a
  `vitest-evals` `HarnessRunError`. The Rust harness error type does not carry partial runs, so the
  success-path artifact merge is ported and the error-attachment branch is a gap.
- `repetitions` is `u32` (positive integer domain) instead of an arbitrary JS number; non-integer and
  unsafe-integer values are typed out.

## Discrepancies

- None.
