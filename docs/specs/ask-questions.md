# Structured questions

Frozen `core/tools/ask-questions.ts`, `ask-questions-panel.ts`, and `extensions/agents-core/src/question-relay-tools.ts` at `4f9e6f8a4` govern user questions and child-parent question relay.

## What it must do

- [x] Reject omitted/nonboolean blocking before UI availability and question validation; options are optional for free text.
- [x] Register child-aware ask_questions and answer_question with exact frozen metadata and tool order (12 tools).
- [x] Relay child requests to the direct parent with child-scoped request IDs; nonblocking returns queued, blocking cancellation retains the sent request.
- [x] Route parent answers only to active direct children in the same persisted control store; blocking claims only its exact child/request and returns durable-acceptance metadata.
- [x] Persist tool-result mailbox receipts before deleting claimed answers; do not append the tool result twice.
- [x] Serialize panels per UI, recover from panel rejection, and enqueue nonblocking answers/cancellation correlated by requestId.
- [x] Match frozen tab navigation, free text, Other, multi-select source-order formatting, partial cancellation, submission, and rendering.

## How it works

- [Core traceability](../porting/traceability/ask-questions.md)
- [Relay traceability](../porting/traceability/question-relay-tools.md)
- [Panel traceability](../porting/traceability/ask-questions-panel.md)

## Implementation inventory

- `src/core/tools/ask_questions.rs` — explicit blocking, validation, per-UI FIFO and queued inbox delivery.
- `src/core/tools/ask_questions_panel.rs` — UI-local tabbed panel.
- `src/bundled_extensions/agents_core/question_relay_tools.rs` — parent request and direct-child answer routing.
- `src/core/agent_session.rs` — tool-result receipt acceptance and finally release.

Paths above are relative to `crates/pi-coding-agent/`.

## Tests asserting this spec

- `tests/question_relay.rs`
- `tests/ask_questions_tool.rs`
- `tests/differential_agents_core_deterministic.rs::registration_metadata_matches_the_pinned_typescript_registration`
- `tests/differential_session_tools.rs::ask_questions_tool_differential`
- `tests/agent_session_runtime_mailbox.rs::question_tool_result_persists_receipt_before_consuming_answer`

## Known gaps (current cycle)

- [ ] Production mutants and remaining malformed/late-reply branches pending.
- [ ] Exact source malformed-answer branch and late reply/cancellation race need additional proof.

## Out of scope

- Live desktop/control state; every test runs through the mandatory Cargo isolation runner.
- Whole agents runtime parity is not implied by exact registration metadata.
