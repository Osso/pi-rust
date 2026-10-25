# Goal scheduling — frozen 4f9e6f8a4

| Surface | Native map / proof | Status |
| --- | --- | --- |
| captureEpoch/isEpochCurrent/clearSession/clearAll | Per-session epochs; clear aborts pending decisions, reviews and agent wait controllers. | wider branch proof pending |
| waitForAgentsOrScheduleReview | list error/zero/active branches; active waits receive the same scheduled reviewAt and a concurrent **25-minute** review timer. Old native five-minute delay removed. | timeout branch proven |
| scheduleWaitReview/claimWaitReview | Timer and agent wake compete for one epoch. Winner increments epoch and aborts the other path before reviewing. Native running timer handle is removed before claiming so it cannot abort its own async continuation. | frozen/native timeout-win matrix GREEN |
| waitForAgentWake/startAgentWait | Aborted/stale responses ignored, tool errors explicit, wake callback before review, wake content retained. Errors report against original reviewAt without rescheduling. | broader wake/error proof pending |
| deferReview/deferDecision/retry/apply | Same-goal, idle/pending, epoch recheck, error paths retained; native context failures propagate rather than defaulting. | broader proof pending |
| reportError failure | Frozen probe observes `unhandledRejection` with the exact status-append error from detached wait catch. Root native adapter uses shared process-uncaught authority for this boundary only. | probe added |
| Runtime exclusions | Timer/microtask races, numeric overflow and exotic wrong-typed tool detail shapes follow the repository gap policy. | gap |

`goal_scheduling_review_timeout_wins_over_a_live_agent_wait` uses paused Tokio time with a real outstanding wait future, checks no review after five minutes, advances to25 minutes, then delivers the late wake. The frozen fixture runs its actual scheduler with a fake clock/timer and the same ordering; events match exactly.

## Successor branch closure

The entire frozen file was compared against the native scheduler. Additional
`goal_scheduling_wait_wake_error_and_clear_match_frozen` scenarios prove no active
agents, list error, wait error retaining the original deadline, successful wake
with null details/content evidence, changed goal, and clear-all before late wake.
`goal_agent_end_cancellation_and_deferred_paths_match_frozen` also proves deferred
review/decision busy retry and epoch rejection; the completion matrix proves
session clear/guard invalidation. Combined with the existing 25-minute timeout
winner and report-error probe, these close the realistic scheduling branches.
All tests GREEN in `/tmp/bundled3-goal-final-matrices.log` (35 goal tests).
The gap row for runtime races/numeric extremes above remains; no new semantic gap.
