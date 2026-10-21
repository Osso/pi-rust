# Question relay tools

Frozen `packages/coding-agent/extensions/agents-core/src/question-relay-tools.ts` at `4f9e6f8a4`; native `bundled_extensions/agents_core/question_relay_tools.rs`.

| Frozen symbols / branches | Native | Proof |
|---|---|---|
| registerQuestionRelayTools, createChildAwareAskQuestionsTool | register_question_relay_tools | exact full 12-tool metadata differential; child/main dispatch |
| runtime.registerAgentsMailboxTools includes both relay tools | runtime::register_agents_mailbox_tools | standalone registration regression (was 2 vs frozen 4); same shared store and source insertion order |
| isChildRuntime, explicit boolean check | context identity/requires-ID; parse_params | missing_identity_explicit_blocking_and_direct_child_route_are_enforced |
| answerMessageId, formatQuestion, formatParentQuestionRequest, sendQuestionsToParent | same private helpers, existing contact_parent transport | child_nonblocking_relays_formatted_request_and_correlates_answers; child-blocking cancellation |
| askParent nonblocking / cancellation | ask_parent, wait_for_answer | queued/requestId; sent request survives cancellation |
| takeQuestionAnswer, waitForQuestionAnswer, deliveredAnswerResult | child/request mailbox predicate, 500ms cancellation-aware poll, runtimeMailboxDelivery | blocking_reply_claims_only_child_and_request_and_returns_acceptance_descriptor |
| readAnswerRoute, answerQuestion | active direct-child/store validation and enqueue_stored_runtime_mailbox_message | direct-child rejection, correlated recipient session and JSON; inactive/unavailable branch proof pending |
| parseQuestionAnswer / formatAnswers with unchecked fields | lossless JS JSON parser, discriminator, JS truthiness/enumeration/string coercion | six malformed/unchecked payload differential fixtures; parser error RED reproduced serde-vs-V8 error text; all six unchecked-payload cases GREEN |
| durable tool result acceptance | AgentSession MessageEnd captures descriptor before extensions; accept/finally release | question_tool_result_persists_receipt_before_consuming_answer |

Six whole-module frozen differential requests cover nonblocking request formatting, cancellation, free-text/multi-choice formatting, explicit-blocking and missing-identity validation. Real SQLite tests separately prove answer delivery/correlation.

No eager answer deletion; receipt integration uses the shared frozen mailbox acceptance boundary. Source discrepancy: the QuestionAnswer cast validates only the type discriminator, not answers/cancelled. Preserve absent cancelled, truthy nonboolean cancellation, Object.entries null rejection and V8 JSON parse errors rather than adding validation. Six fixtures drive that behavior; source-typed valid Unicode remains the supported text boundary. Parent classified lone-surrogate answer fields as an explicit unapproved gap, NOT a typed-core exclusion (2026-10-21). The upcoming lossless shared TextContent/details lane must remove repairs at delivered_answer_result (answers/cancelled into_serde_lossy), format_answer_payload (question/value to_utf8_lossy), and core ask_questions::question_result (serde Value details / String text_result). No full UTF-16 parity claim. Late-answer 500ms-poll and inactive/unavailable/transport-error fixtures are GREEN. Five production mutants (request predicate, child authorization, queued requestId, receipt bypass and 26-minute deadline) each failed behavioral assertions; all source restored.
