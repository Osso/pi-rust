# Core ask questions

Frozen `packages/coding-agent/src/core/tools/ask-questions.ts` at `4f9e6f8a4`; native `core/tools/ask_questions.rs`. Retires historical select/input-dialog behavior and old-pin proof.

| Frozen symbols / branches | Native | Proof |
|---|---|---|
| schema, definition/guidelines/approval/execution mode | parameters, create_ask_questions_tool_definition | full agents-core registration differential; approval false, exact metadata |
| explicit blocking validation before UI availability | parse_params / execute | omitted-blocking differential plus real missing-context RED→required blocking guard before context lookup; source/native3 histories in differential_question_guards |
| validateQuestions / validateQuestionOptions, optional free-text options | validate_questions | duplicate questions/options differential; multiSelect-without-options proof pending |
| queueQuestions / showQuestions; rejection doesn't strand next request | UI-identity queue/worker; custom panel | failed_panel_does_not_strand_later_requests; agent_abort_keeps_pending_panel_fifo_until_settlement reproduces agent-core abort dropping the tool future and proves settlement precedes next panel (GREEN; weak-job ownership mutant killed); 12 omitted/explicit frozen core cases |
| captureQuestionInboxTarget / startQueuedQuestionRequest / deliverQueuedAnswers / enqueueQuestionAnswer | capture_inbox / deliver_queued | nonblocking_answers_and_cancellation_arrive_via_correlated_inbox; real persisted manager/empty DB path RED question-guards-red.log: source rejects before queue, native queued. Native now applies literal falsy path guards; failure delivery proof pending |
| answered/cancelled/unavailable result details + metadata | question_result, execute | baseline and expanded core differential; expanded differential passes |
| formatAnswers | format_answers with JavaScript object_entries | numeric question-key answer-text RED/GREEN regression; source enumeration order retained |
| notification helpers | notify_ask_questions_waiting / close_ask_questions_notification | common-pin Result<_,CallbackError> notifier/handle prerequisite; source296-307 catches log send/close failures and continue. fallible_notifications covers original throw retention and catch settlement; UI notify-throw migration remains pending crate split |
| renderCall/renderResult; answers stay on call row | call_text / render_call / render_result | actual frozen retained-result RED question-render-history-red-2.log: native cleared an existing result Text while source returns it unchanged. Native now updates only the call row, retains any result component, and uses fallible Text call rendering.16-frame exact-color/raw-text/queued/cancelled/stale-details/reuse/type-error history; final proof pending |

## Q18 native queue/render checkpoint

Question queue results/errors, captured DB/session paths, queued request/message IDs, notification bodies and rendered call rows retain `JsString`. The provider tool-call ID is widened once into the queued native request. Native UI/custom errors pass through one channel carrier; only stderr reporting encodes text. Question/answer text is never encoded before theme rendering.

`raw_queue_tests::queued_answer_body_keeps_raw_request_question_and_answer_units` covers the pure queued-answer formatter. It and the current panel/tool integration suites remain unexecuted pending coding-library compilation. Earlier proofs below do not establish acceptance of this checkpoint.

Real panel results and initial/post-key text layouts match frozen in eight keystroke scenarios, including inherited `__proto__` string assignment producing no own answer. Notification/error and tool renderer proof gaps remain; no whole-module completion claim. [Tabbed panel](ask-questions-panel.md), [relay](question-relay-tools.md), [contract](../../specs/ask-questions.md).
