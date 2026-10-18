# coding-agent ask-questions traceability

Pinned source: `packages/coding-agent/src/core/tools/ask-questions.ts` at `532f7d12`.

Rust: `crates/pi-coding-agent/src/core/tools/ask_questions.rs` (`AskQuestionsTool`).

| Source | Rust | Tests |
|---|---|---|
| `askQuestionsSchema` with min/max constraints | `parameters()` | `exposes_min_max_schema_constraints`, differential |
| `validateQuestions` (count, duplicate questions/options, option range) | `validate_questions` | `rejects_duplicate_questions`, `rejects_duplicate_option_labels` |
| `formatOption` (em dash + description) | `format_option` | single-choice differential |
| `buildSelectionOptions` (automatic `Other`) | `build_selection_options` | `adds_an_automatic_other_option_for_custom_answers` |
| `askCustomAnswer` (`ui.input` + trim) | `ask_custom_answer` | custom-answer test |
| `askSingleQuestion` (`ui.select`, Cancel, unmatched choice) | `ask_single_question` | single-choice/cancel tests |
| `formatMultiSelectOption`, toggle, custom add, Done join | `format_multi_select_option`, `apply_multi_select_choice`, `ask_multi_select_question` | `supports_multi_select_questions` |
| `formatAnswers` (`- question → answer`, `(no selection)`) | `format_answers` | result-text test |
| `unavailableResult` / `cancelledResult` / `answeredResult` | `unavailable_result`, `cancelled_result`, `answered_result` | TUI-unavailable, partial-cancel tests |
| desktop notification body/title/expiry/urgency and close | `format_ask_questions_notification_body`, `notify_ask_questions_waiting`, `close_ask_questions_notification` | notification and close tests; notifier injected via `ToolContext` |
| notification failure handling (`console.error`) | `DesktopNotifier::notify` returning `None` | system notifier has no error channel; gap below |

Differential: `tests/differential_session_tools.rs::ask_questions_tool_differential` with a
scripted UI and a fake `notify-send` on `PATH` (single, Other, multi-select, cancel,
duplicate options, duplicate questions).

## Gaps

- `renderCall`/`renderResult` need `Theme`.
- The source wraps `sendDesktopNotification` in `try/catch` and logs to `console.error`; the Rust
  `DesktopNotifier` trait has no error channel, so a throwing host notifier cannot be represented.
  Notification content/close behavior is identical.
