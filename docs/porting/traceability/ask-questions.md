# Core ask questions

Frozen `packages/coding-agent/src/core/tools/ask-questions.ts` at `4f9e6f8a4`; native `core/tools/ask_questions.rs`. Retires historical select/input-dialog behavior and old-pin proof.

| Frozen symbols / branches | Native | Proof |
|---|---|---|
| schema, definition/guidelines/approval/execution mode | parameters, create_ask_questions_tool_definition | full agents-core registration differential; approval false, exact metadata |
| explicit blocking validation before UI availability | parse_params / execute | omitted-blocking differential GREEN; explicit_boolean_is_required_before_ui_or_question_validation |
| validateQuestions / validateQuestionOptions, optional free-text options | validate_questions | duplicate questions/options differential; multiSelect-without-options proof pending |
| queueQuestions / showQuestions; rejection doesn't strand next request | UI-identity queue/worker; custom panel | failed_panel_does_not_strand_later_requests; agent_abort_keeps_pending_panel_fifo_until_settlement reproduces agent-core abort dropping the tool future and proves settlement precedes next panel; 12 omitted/explicit frozen core cases |
| captureQuestionInboxTarget / startQueuedQuestionRequest / deliverQueuedAnswers / enqueueQuestionAnswer | capture_inbox / deliver_queued | nonblocking_answers_and_cancellation_arrive_via_correlated_inbox passes; failure delivery proof pending |
| answered/cancelled/unavailable result details + metadata | question_result, execute | baseline and expanded core differential; expanded differential passes |
| formatAnswers | format_answers with JavaScript object_entries | numeric question-key answer-text RED/GREEN regression; source enumeration order retained |
| notification helpers | notify_ask_questions_waiting / close_ask_questions_notification | retained system notifier API; error-return trait mismatch remains (cannot model throwing custom notifier) |
| renderCall/renderResult; answers stay on call row | call_text / render_call / render_result | implementation present; render differential refresh pending |

Real panel results and initial/post-key text layouts match frozen in eight keystroke scenarios, including inherited `__proto__` string assignment producing no own answer. Notification/error and tool renderer proof gaps remain; no whole-module completion claim. [Tabbed panel](ask-questions-panel.md), [relay](question-relay-tools.md), [contract](../../specs/ask-questions.md).
