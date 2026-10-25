# Scheduled goal agent-end — frozen 4f9e6f8a4

Whole export/branch audit: `runScheduledGoalAgentEnd` captures the review epoch before calling `handleGoalAgentEnd`, forwards event/context/review/select/same-goal/apply, and wires epoch/deferDecision/deferReview to the same scheduler. The supplied empty-response scheduler is intentionally unused in frozen. Native wrappers now propagate Result from all context/scheduler callbacks instead of defaulting failure to false. Existing goal agent-end integration tests cover normal decisions; explicit cancellation/deferred callback matrix remains pending, so the row is partial. Runtime races and pathological input shapes follow the repository gap policy.

## Successor whole-wrapper proof

`goal_agent_end_cancellation_and_deferred_paths_match_frozen` uses real schedulers
in TS and Rust, compares no goal, immediate apply, changed goal, epoch cancellation
during review, pending-before-review, and pending-after-review. Deferred cases
first fire while busy and then apply/review when idle with no pending messages.
No stub of native epoch/timer behavior. All six scenarios GREEN.
Scheduling interleavings outside this controlled ordering are a repository-policy gap.
