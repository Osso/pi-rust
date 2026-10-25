# idle-compaction.ts

Frozen common pin4f9e6f8a4, `packages/coding-agent/src/core/idle-compaction.ts`1–33. Native `core/idle_compaction.rs`; [spec](../../specs/idle-compaction.md).

| Source | Native | Proof |
|---|---|---|
| computeIdleCompactionDueAt, supported APIs/TTL/minimum/context sum | compute_idle_compaction_due_at | differential_idle_compaction:24 explicit API/token/timestamp actual-frozen cases |
| Fractional/negative timestamps, token boundary99999/100000/100001; output/cache components, totalTokens ignored | binary64 calculation | same24-case matrix |
| Plain-object inherited TTL lookup (:29–33) | Unsupported APIs return None | **gap**: approved runtime-oddity exclusion (2026-10-23); own_key_tables native regression |
| AgentSession4736–4772 schedule/find/run | schedule_idle_compaction/find_idle_compaction_target/run_idle_compaction | agent_session idle tests4 GREEN, real auto-compaction |
| AgentSession2693,5197,2269 finalization/start/reload/disposal | production lifecycle call sites/timer cancellation | finalization/disposal exercised; dedicated restart/reload proof pending |
| target.message reference guard and branch-order boundary | Agent token + SessionManager entry-token lookup; timestamp only when no shared reference exists | equal-valued replacement rejection, manual-boundary rejection; stored identity differentials in agent_identity_paths |

## Current helper proof

2026-10-25 whole 33-line common-pin helper comparison and surviving 24-case frozen differential GREEN at 2683e5cbc: `cargo test -j3 -p pi-coding-agent --test suite_session_1 differential_idle_compaction` (`/tmp/ext-api-suite_session_1-differential_idle_compaction.log`). This closes the helper row; AgentSession lifecycle proofs below remain a separate consumer scope.

## Approved inherited-table gap

Frozen TTL map leaks inherited Object.prototype values, yielding NaN deadlines. The 2026-10-23 runtime-oddity exclusion removes this emulation and its exclusive 1ms scheduler test. Native supports only explicit API keys. [Policy](../../specs/object-prototype-lookups.md). Historical 96-case proof below predates this removal; surviving 24-case differential covers all explicit API branches.

## Proof scope

Historical `q15-idle-helper-green-1.log`: helper96 cases GREEN before runtime-oddity removal. `q15-idle-scheduler-green-4.log`: scheduler4 GREEN covering two provider deadlines/once-only, unsupported API/small context/settings/reference rejection, queued input/model change/disposal/manual compaction rejection and inherited API coercion. Scenarios sharing API names serialize access to the process-global fake provider registry; production ordering is unchanged. Earlier batch failure was reproduced as a cross-scenario registration collision; the same isolated case passed.

Following lifecycle tests add startup/reload restored-context scheduling, observer/error exclusion and aborted eligibility, busy-tool crossing and latest-request timestamp replacement. Composite current proof: six cases GREEN in q15-idle-lifecycle-green-2.log; the restored/reload test GREEN after correcting both required fake summary responses and re-registering its fake transport at the source reload hook (q15-idle-restore-green-2.log). No production change. Restoration here reconstructs manager/Agent state in-process; source crash/restart scenario still needs process-boundary proof. Final lane gates are not claimed by this slice.
