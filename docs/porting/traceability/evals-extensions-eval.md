# evals/src/extensions.eval.ts

Source: `packages/evals/src/extensions.eval.ts` (140 lines). Rust:
`crates/pi-evals/src/evals/extensions.rs`; eval body:
`crates/pi-evals/tests/extensions_eval.rs`.

## Symbols and branches

| TypeScript | Rust | Tests |
|---|---|---|
| `createExtensionAuthoringHarness` | `create_extension_authoring_harness` (base options carry the runner model/runtime) | `harness_table_runs_the_extension_authoring_eval` |
| output transform: response, `systemPromptHasGuidelines`, `systemPromptHasPiDocs`, `extensionErrors`, `loadedExtensions` (`tools.keys()`), `extensionSource` | `extension_authoring_output` | same |
| `excludeGuidelinesAndDocumentation` | `exclude_guidelines_and_documentation` | `system_prompt_transforms_match_the_source` |
| `prepareDefaultPromptOverride` | `prepare_default_prompt_override` | same (including `lastIndexOf` semantics) |
| `ExtensionAuthoringJudge` | `extension_authoring_judge` | success + 10 failure fixtures + multi-failure join |
| `evalHarnessTable("Pi extension authoring system prompt", { baseline, candidate })` | `extension_harness_table` | `harness_table_runs_the_extension_authoring_eval` |
| eval body: prompt → reload → prompt; source artifact; prompt-shape assertions | integration test body | same |

Judge branches covered with concrete fixtures: missing source; missing canonical import;
`@mariozechner/` import; `@sinclair/typebox` import; loader errors; no `hello` tool; pending call;
wrong argument; wrong result; wrong final response; failure ordering and `; ` join.

## Gaps

- The source artifact is only recorded when the model actually writes
  `<cwd>/.pi/extensions/hello.ts`. Faux runs do not create the file, so the artifact-recording path
  is covered by `record_eval_source_artifact` tests plus the opt-in live eval.
- `judgeThreshold: null` is Vitest scoring configuration; the Rust test calls the judge directly and
  asserts the score/rationale instead of failing the run.

## Invocation and live-model gating

The Vitest runner is replaced by Rust integration tests. The live eval
(`extension_authoring_eval_requires_a_live_model_when_opted_in`) runs only when both `PI_PROVIDER`
and `PI_MODEL` are set, mirroring the TypeScript runner gate, and uses the normal
`ModelRuntime::create()` credential store. Without those variables it prints a skip notice and does
not touch provider state.

## Discrepancies

- Same `end_turn` stop-reason discrepancy as `evals-pi-harness.md`: a compliant live model that
  answers the `end_turn` continuation with `toolUse` makes the harness throw. The source eval body
  has the same failure mode; ported faithfully.
