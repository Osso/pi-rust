# evals/src/smoke.eval.ts

Source: `packages/evals/src/smoke.eval.ts` (17 lines). Rust: `crates/pi-evals/src/evals/smoke.rs`;
eval body: `crates/pi-evals/tests/smoke_eval.rs`.

## Behavior

| TypeScript | Rust | Test |
|---|---|---|
| `createPiCodingAgentHarness({ noTools: "all" })` | `create_smoke_harness` (`no_tools = NoTools::All`) | `smoke_eval_runs_a_basic_prompt_end_to_end` |
| `run("What's the capital of France?...")` | `PiCodingAgentInput::Prompt` | same |
| `result.output.trim() === "Paris"` | `js_trim` comparison | same |
| `result.errors === []` | `run.errors` empty | same |
| `usage.provider === PI_PROVIDER`, `usage.model === PI_MODEL`, `totalTokens > 0` | same fields | same (faux values) |
| runner gating error | `resolve_model_selection` | `smoke_eval_requires_a_model_selection` |

## Invocation and live-model gating

The Vitest runner is replaced by Rust integration tests. `smoke_eval_requires_a_live_model_when_opted_in`
runs only when both `PI_PROVIDER` and `PI_MODEL` are set (the TypeScript runner gate), uses
`ModelRuntime::create()` with the normal credential store, and asserts the source expectations.
Without the variables it prints a skip notice and does not touch provider state.

## Gaps

- `noTools: "all"` does not remove `end_turn` (the runtime keeps it always active); the faux test
  supplies a terminal `end_turn` turn, matching the source's expected terminal shape.

## Discrepancies

- Same `end_turn` stop-reason discrepancy as `evals-pi-harness.md`.
