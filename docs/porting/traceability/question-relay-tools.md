# Question relay tools

Frozen `packages/coding-agent/extensions/agents-core/src/question-relay-tools.ts` at `4f9e6f8a4`; native `bundled_extensions/agents_core/question_relay_tools.rs`.

| Frozen symbols / branches | Native | Proof |
|---|---|---|
| registerQuestionRelayTools, createChildAwareAskQuestionsTool | register_question_relay_tools | exact full 12-tool metadata differential; child/main dispatch |
| isChildRuntime, explicit boolean check | context identity/requires-ID; parse_params | missing_identity_explicit_blocking_and_direct_child_route_are_enforced |
| answerMessageId, formatQuestion, formatParentQuestionRequest, sendQuestionsToParent | same private helpers, existing contact_parent transport | child_nonblocking_relays_formatted_request_and_correlates_answers; child-blocking cancellation |
| askParent nonblocking / cancellation | ask_parent, wait_for_answer | queued/requestId; sent request survives cancellation |
| takeQuestionAnswer, waitForQuestionAnswer, deliveredAnswerResult | child/request mailbox predicate, 500ms cancellation-aware poll, runtimeMailboxDelivery | blocking_reply_claims_only_child_and_request_and_returns_acceptance_descriptor |
| readAnswerRoute, answerQuestion | active direct-child/store validation and enqueue_stored_runtime_mailbox_message | direct-child rejection, correlated recipient session and JSON; inactive/unavailable branch proof pending |
| parseQuestionAnswer malformed type | type discriminator check | implementation present; malformed payload differential pending |
| durable tool result acceptance | AgentSession MessageEnd captures descriptor before extensions; accept/finally release | question_tool_result_persists_receipt_before_consuming_answer |

No eager answer deletion; receipt integration uses the shared frozen mailbox acceptance boundary. Source malformed JSON/type and answer failure cases remain explicit proof gaps, not approved deviations.
