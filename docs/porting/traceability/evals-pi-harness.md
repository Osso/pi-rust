# evals/src/pi-harness.ts

Source: `packages/evals/src/pi-harness.ts` (257 lines). Rust: `crates/pi-evals/src/pi_harness.rs`.
Tests: `crates/pi-evals/tests/pi_harness.rs` (faux provider), `crates/pi-evals/tests/differential_pi_harness.rs` (model selection).

## Exported symbols

| TypeScript | Rust | Tests |
|---|---|---|
| `PiCodingAgentInput` | `PiCodingAgentInput` (`Prompt`, `Steps`) | `differential_pi_harness::eval_inputs_serialize_to_the_typescript_shape`, `pi_harness::harness_runs_prompt_and_reload_steps_in_order` |
| `resolveModelSelection` | `resolve_model_selection` | `differential_pi_harness::resolve_model_selection_matches_the_typescript_module` (13 fixtures: explicit preference, trimming, missing pairs, empty/whitespace values, JS `\u00a0` trim) |
| `createPiCodingAgentHarness` | `create_pi_coding_agent_harness` / `PiCodingAgentHarness` | `pi_harness` suite |

## Branches

| Branch | Rust | Test |
|---|---|---|
| `startedAt` / `performance.now()` timings | `Instant` → `TimingSummary.total_ms` | `harness_runs_a_prompt_with_a_faux_provider` |
| `signal?.throwIfAborted()` | `throw_if_aborted` → `PiEvalError::Abort("This operation was aborted")` | `harness_reports_abort_before_start` (name `AbortError`) |
| `resolveModelSelection` | `resolve_model_selection(options.model, PiModelEnvironment::from_process())` | differential suite |
| `ModelRuntime.create()` | `ModelRuntime::create(CreateModelRuntimeOptions::default())`; `model_runtime` option is the Rust carrier | faux suite injects a runtime |
| `getModel` missing → `Eval model not found: p/m` | same message | `harness_reports_a_missing_model` |
| `mkdtemp(join(tmpdir(), "pi-eval-"))` + `workspace`/`agent` dirs | `tempfile` prefix `pi-eval-` + `workspace`/`agent` | success/tool/transform tests |
| `createAgentSessionServices` with in-memory settings | `create_agent_session_services` + `SettingsManager::in_memory` | success test |
| `systemPromptOverride: () => transformedSystemPrompt` | `Arc<Mutex<Option<String>>>` override closure, set after session creation, then `reload(None)` | `harness_applies_the_system_prompt_transform` |
| empty transformed prompt → `Transformed eval system prompt must not be empty.` | same message | `harness_rejects_an_empty_transformed_system_prompt` |
| abort listener → `session.abort()` | spawned task on `CancellationToken`, awaited after the run | not exercised in-flight (no deterministic faux hook); start check tested |
| extensions present → `Expected an isolated eval session to start without extensions.` | same message | unrepresentable in tests: the default loader always exposes the built-in `end_turn` tool, not an extension path |
| `promptAgent` empty assistant → `Agent run completed without an assistant message.` | same message | reachable only by runtime message removal (length recovery); branch ported |
| `stopReason !== "stop"` → `errorMessage` or `Agent run ended with unexpected stop reason: <reason>.` | `stop_reason_label` + same messages | `harness_reports_unexpected_stop_reasons` (`aborted`, provider `errorMessage`) |
| no assistant text → `Agent run produced no assistant text.` | same message | ported; covered indirectly |
| `transformSystemPrompt` throw | `PiSystemPromptTransform` returns `Result` | transform test + `system_prompt_transforms_match_the_source` (extensions) |
| `output` transform | `PiHarnessOutputFn` (`BoxFuture`) | transform/output-error tests |
| usage: provider/model/input/output/total/toolCalls | `UsageSummary` from `SessionStats` | success test |
| `hasPricing` over `model.cost` + tiers | `model_has_pricing` | success test asserts `estimatedCostUsd` |
| metadata: `cacheReadTokens`, `cacheWriteTokens`, `estimatedCostUsd` | same keys/order | success test |
| `toTranscriptEvents`: user text | `TranscriptEvent::Message` | success test |
| assistant text only when truthy; tool calls with `normalizeRecord(arguments)` | same | success/tool tests |
| tool result text vs JSON content; error `text || "Tool failed"` | same | tool test (error path) |
| `setArtifact("runId", sessionId)` | same | success test |
| session snapshot before removing the root | `PI_SESSION_SNAPSHOT_ARTIFACT` artifact | success test |
| `session.dispose()` + `rm(root)` cleanup errors | `dispose` + `TempDir::close`, aggregated | ported; cleanup-failure paths have no deterministic trigger |
| `AggregateError` messages | `PiEvalError::Aggregate` with the same messages | ported; no deterministic trigger |

## Gaps

- `HarnessContext` is passed by value; the run's artifacts are exposed on `HarnessRun.artifacts`
  (the source's observable contract) instead of mutating a caller-owned bag. `withIterationArtifact`
  merges the wrapper context with the returned run artifacts.
- `createPiCodingAgentHarness` overloads become one `PiCodingAgentHarnessOptions` value; `output`
  transforms return `serde_json::Value` (`JsonValue` in the source).
- OpenTelemetry trace attachment (`ensureRunTrace`) belongs to the third-party `vitest-evals`
  `createHarness` wrapper; the Rust harness returns the normalized run directly.
- `createHarness` failed-run attachment (`attachHarnessRunToError`) is not modeled; the Pi harness
  never produces partial runs on error.
- `normalizeRecord`/`toJsonValue` are replaced by typed serialization because the Rust input domain
  is already JSON-safe (`ToolCall.arguments: Map<String, Value>`, tool result content
  `serde_json::to_value`).

## Discrepancies

- `promptAgent` requires the final assistant message to have `stopReason === "stop"`. The
  coding-agent runtime always keeps `end_turn` active (even with `noTools: "all"`), so a text-only
  answer triggers the `end_turn` continuation; a provider that reports `toolUse` for the resulting
  `end_turn` call makes the harness throw `Agent run ended with unexpected stop reason: toolUse.`.
  Ported faithfully; faux tests use the source's expected terminal shape (text + `end_turn` with
  `stopReason: "stop"`).
- `signal?.throwIfAborted()` throws a `DOMException` named `AbortError`; `PiEvalError::Abort`
  carries the name so callers can distinguish it.
