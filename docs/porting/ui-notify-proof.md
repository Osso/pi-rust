# UI notification and event-bus checkpoint

Reference: unmodified `4f9e6f8a4`. Implementation candidate: `5fb036af`; compact prerequisite: `a43a6c76` plus fixture adapter `aff2678a`.

| Capability | Evidence |
|---|---|
| UI notification Error/code/raw/null rejection retention | `fallible_notifications`: 3 tests; interface RED precedes migration |
| Original event-listener throws, logging and later delivery | `differential_event_bus_errors`: 4 frozen histories; `event_bus`: 9 retained tests |
| Queued notification throw reaches outer catch, creates no failure record, next answer keeps captured session | Actual frozen/native child differential: `question-notify-throw-first.log` |
| Existing consumer behavior after propagation | 16 retained tests across bwrap, codex-usage, DOOM, GitHub issues and trigger-compact; `ui-notify-consumer-regressions.log` |
| Compact completion, completion rejection routed to error, error rejection escapes | 3 actual-source histories; 10 tests, ignored child helper explicitly exercised; `compact-callback-histories-green.log` |

Artifacts are under `/tmp/pi-rust-repin-core-tools/`. Workspace all-target keep-going check: 0 errors/warnings (`ui-notify-check-final-summary.json`). Four-package all-target/all-feature warning Clippy: 0 changed-primary-span findings; 1666 existing global diagnostics remain (`ui-notify-clippy-clean-summary.json`). The final three conversion/borrow removals are also compiled by that Clippy command.

Protected strict TypeScript checks pass for 7 fixtures using the common unmodified reference (`ui-notify-final-types.log`). Biome's general session-tools oracle retains its existing 23 warnings and formatter error; it is byte-identical to the pre-migration merge point. Six new/specialized fixtures are clean. Manual changed-line Rust/TS readability audit found no new suppression, carrier, fallback or deep branch. Format passes before each commit. Inventory passes: 686 rows / 186679 lines, 451 done / 19 gap / 48 partial / 168 todo.

This checkpoint does not close whole ask-questions/panel parity. Its separate approved prerequisite must preserve `custom()` rejections through the question queue/job/receiver and blocking result. Other UI APIs and request-future rejection semantics are unchanged. Local catch placement was audited against frozen consumers; the retained consumer tests do not inject every possible notification throw.

[Contract](../specs/fallible-ui-notifications.md), [event bus](traceability/event-bus.md), [questions](traceability/ask-questions.md).
