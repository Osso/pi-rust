# Core ask questions

Frozen `packages/coding-agent/src/core/tools/ask-questions.ts` at `4f9e6f8a4`; native `core/tools/ask_questions.rs`. Retires historical select/input-dialog behavior and old-pin proof.

| Frozen symbols / branches | Native | Proof |
|---|---|---|
| schema, definition/guidelines/approval/execution mode | parameters, create_ask_questions_tool_definition | full agents-core registration differential; approval false, exact metadata |
| explicit blocking validation before UI availability | parse_params / execute | omitted-blocking differential GREEN; explicit_boolean_is_required_before_ui_or_question_validation |
| validateQuestions / validateQuestionOptions, optional free-text options | validate_questions | duplicate questions/options differential; multiSelect-without-options proof pending |
| queueQuestions / showQuestions; rejection doesn't strand next request | UI-identity queue/worker; custom panel | failed_panel_does_not_strand_later_requests passes; expanded frozen differential pending |
| captureQuestionInboxTarget / startQueuedQuestionRequest / deliverQueuedAnswers / enqueueQuestionAnswer | capture_inbox / deliver_queued | nonblocking_answers_and_cancellation_arrive_via_correlated_inbox passes; failure delivery proof pending |
| answered/cancelled/unavailable result details + metadata | question_result, execute | baseline and expanded core differential; expanded rerun pending |
| formatAnswers | format_answers | child scoped blank answer and panel results |
| notification helpers | notify_ask_questions_waiting / close_ask_questions_notification | retained system notifier API; error-return trait mismatch remains (cannot model throwing custom notifier) |
| renderCall/renderResult; answers stay on call row | call_text / render_call / render_result | implementation present; render differential refresh pending |

No whole-module completion claim until pending behavioral/renderer proof. [Tabbed panel](ask-questions-panel.md), [relay](question-relay-tools.md), [contract](../../specs/ask-questions.md).
