# Goal review evidence traceability

Source: `packages/coding-agent/extensions/goal/src/goal-review-evidence.ts`, common pin `4f9e6f8a4`. Rust: `crates/pi-coding-agent-extensions/src/bundled_extensions/goal/goal_review_evidence.rs`.

| Source symbol / branches | Native mapping / proof |
|---|---|
| `appendEvidence`: absent active goal no-op; otherwise append without replacing prior events | `append_evidence`; frozen snapshots before/after goal creation and tool/input events |
| `appendInputEvidence`: extension/empty ignored, raw user text retained | `append_input_evidence`; extension, empty, interactive/rpc and raw UTF-16 inputs |
| `appendEndTurnEvidence`: end_turn success only, nonblank string reason retained untrimmed | `append_end_turn_evidence`; valid/blank/error/other-tool cases |
| `answeredQuestionsText`: ask_questions success with details, not queued/cancelled; text parts join with newline | `answered_questions_text`; queued/cancelled/error/null/no-text/mixed-text-image cases |
| `consumeEvidence`: zero-count no-op, same createdAt/objective only, slice prefix, remove empty field | `consume_evidence`; late evidence retained, explicit zero count, full consumption and replaced goal |
| `createEvidenceReview`: add nonempty conversationEvents, capture evidenceCount, preserve payload fields | `create_evidence_review`; exact review payload/result snapshots with stale preexisting keys |
| exported controller input wrapper keeps latest request and clears prior answers, even without a goal | per-session `UserEvidence`; request-before-goal and new request after answers |
| exported tool wrapper accumulates blocking answers both as review context and durable user evidence | controller `append_tool_result`; two answers separated by review/consumption |
| exported review wrapper merges session userRequest/userAnswers before conversationEvents | shared session map plus review closure; full frozen payload equality |

Proof: `suite_extensions::goal_review_evidence::goal_review_evidence_matches_frozen_user_requests_answers_and_consumption` drives two native/frozen sequences (30 steps), comparing goal state, review outputs and all payloads after every step. Native uses the actual readonly session façade and in-memory goal persistence; the oracle uses the real export with its frozen store interface.

RED: `/tmp/bundled-goal-evidence-red.log` reproduces missing user-request/answer context and absent blocking-answer evidence. No scalar projection is used for request, answer, reason or payload text.

Gap: adversarial input shapes and independent async scheduling races follow the shared parity exclusions; returned values use approved owned snapshots.
