# Completion scheduling — frozen 4f9e6f8a4

All exports and branches audited against the frozen file. Native `CompletionWaitScheduler` retains wait/clear/guard methods; fallible context actions return Result instead of being silently defaulted.

| Branch | Native behavior | Proof |
| --- | --- | --- |
| wait/scheduleCompletionReview | Same message, reviewAt and displayId for agent-wait and scheduled status; wake clears countdown for the current session. | integration pending |
| complete/continue/wait/pause | Clear scheduler first; invoke exact callback, retain displayId, consume evidence afterwards. | integration pending |
| error/set | Exact grouped failure status; return without consuming evidence. | integration pending |
| createReviewGuard | Capture epoch once; check live context/epoch on every invocation. | integration pending |
| review payload | objective/completionReport/wakeEvidence forwarded to goal_completion_review. | retained native focused tests; new matrix pending |

This row remains partial until the completion/wake branch matrix is green. Runtime scheduling races and pathological inputs use the repository gap policy.
