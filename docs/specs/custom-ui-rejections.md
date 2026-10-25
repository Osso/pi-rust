# Custom UI rejection transport

## What it must do

- [x] `ExtensionUiContext::custom` returns existing `CallbackError` rejections without replacing native Error branding/properties or arbitrary thrown JSON values.
- [x] Question jobs and pending receivers retain that rejection; blocking exported `createAskQuestionsToolDefinition().execute` propagates it unchanged through `HandlerError`.
- [x] One question execution helper serves both the exported definition and the Rust `ContextTool` adapter. The latter projects runtime error text using frozen `packages/agent-core/src/agent-loop.ts:845-850`: Error.message, otherwise String(value).
- [x] The definition-to-agent execution wrapper uses the same projection. Existing Error text remains unchanged; no shared `ToolError` widening is permitted.
- [x] Queued question failures use Error.message or String(value), matching frozen ask-questions.ts:192-197; notification failures retain their separate outer catch.

## How it works

The existing `CallbackError` crosses UI and queue boundaries. The exported factory bypasses the message-only ContextTool adapter. Text projection occurs only in the runtime adapters; there is no new carrier, panic propagation or uncaught-process authority.

[Question map](../porting/traceability/ask-questions.md), [extension types](../porting/traceability/extensions-types.md), [runtime wrapper](../porting/traceability/tool-definition-wrapper.md).

## Implementation inventory

- `crates/pi-coding-agent-core/src/core/extensions/types.rs`: custom rejection contract.
- `crates/pi-coding-agent-core/src/core/tools/ask_questions.rs`: queue and shared execution helper/direct factory.
- `crates/pi-coding-agent-core/src/core/tools/tool_definition_wrapper.rs`: one execute-rejection projection and its shared helper.
- `crates/pi-coding-agent-interactive/src/modes/interactive/interactive_mode.rs`: matching custom response channel; other custom hosts/callers receive mechanical adaptation only.

## Tests asserting this spec

- `differential_question_rejections`: seven actual frozen direct-factory/agent-loop histories (raw D800 string, TypeError/code/raw message, null, boolean, number, message-lookalike object, missing-context TypeError). Both native ContextTool text and real native agent-loop result match frozen.
- Actual behavioral RED: `question-custom-rejection-red.log`; CallbackError interface RED: `question-custom-callback-api-red.log`. A final direct-factory guard audit exposed Error versus frozen TypeError for missing context: `question-missing-context-brand-red.log`; only that owned guard's existing NativeNodeError constructor changes.
- `question-custom-queue-consumers-green.log`: 15 tests plus explicitly exercised child helper, retaining panel histories, validation/metadata/guards and real durable queue recovery/error boundaries.
- Consumer verification exposed a pre-existing summarize fixture mismatch on exact parent `4c28cdf4` as well as this branch (`summarize-parent-4c28cdf4-baseline.log`): native faux registration omitted the API explicitly set by oracle `extension-examples-i-session-oracle.ts:11`. Main approved that one fixture field to bind the same static gpt-5.2 API even when only model `other` is registered; no summarize or model-registry production logic changes. The same fixture then exposed the shared test helper's hardcoded `api_key="fixture"` overriding empty AuthStorage in the `noAuth` case. A separately approved inline registration omits that field and leaves the shared helper untouched.

## Approved gap: rejection value whose own toString throws

Decision: 2026-10-24 ingress (c). Native `ToolError` cannot carry a loop-fatal outcome. Its adapter returns an explicit ToolError with the conversion TypeError's exact message; it must not silently substitute the original message or exit the process.

The actual unmodified-source probe invokes real `Agent.prompt()` with `{toString:null}` rejected by question UI. Prompt resolves, process exits 0, streaming stops, an assistant error says `Cannot convert object to primitive value`, and no tool-result message exists. Source catch: `agent.ts:498-544`; artifact `question-custom-coercion-source-probe.log`. The native adapter preserves the direct thrown object and exact TypeError text but emits one tool-result message instead of that loop-fatal assistant outcome. `conversion_failure_is_caught_by_frozen_prompt_and_native_reports_the_approved_gap_explicitly` asserts both shapes explicitly.

## Out of scope

Other UI API/factory error contracts, shared ToolError representation, provider behavior, DB semantics, unrelated wrapper changes, deployment and live state. Final all-consumer/static gates remain pending until the prerequisite checkpoint.
