# idle-compaction.ts

Frozen common pin4f9e6f8a4, `packages/coding-agent/src/core/idle-compaction.ts`1–33. Native `core/idle_compaction.rs`; [spec](../../specs/idle-compaction.md).

| Source | Native | Proof |
|---|---|---|
| computeIdleCompactionDueAt, supported APIs/TTL/minimum/context sum | compute_idle_compaction_due_at | differential_idle_compaction:96 actual-frozen cases |
| Fractional/negative timestamps, token boundary99999/100000/100001; output/cache components, totalTokens ignored | binary64 calculation | same96-case matrix |
| Plain-object inherited TTL lookup | Generated INHERITED_TTL_KEYS returns NaN | all12 Object.getOwnPropertyNames(Object.prototype) names read by oracle; undefined/NaN encoded distinctly |
| AgentSession4736–4772 schedule/find/run | schedule_idle_compaction/find_idle_compaction_target/run_idle_compaction | agent_session idle tests4 GREEN, real auto-compaction |
| AgentSession2693,5197,2269 finalization/start/reload/disposal | production lifecycle call sites/timer cancellation | finalization/disposal exercised; dedicated restart/reload proof pending |
| target.message reference guard and branch-order boundary | Agent token + SessionManager entry-token lookup; timestamp only when no shared reference exists | equal-valued replacement rejection, manual-boundary rejection; stored identity differentials in agent_identity_paths |

## DISCREPANCY: inherited table properties

Frozen TTL map is a plain object. Inherited Object.prototype names resolve functions/objects, multiply by0.9 into NaN, then return NaN for contexts at least100000 tokens. They are not unsupported APIs returning undefined. The name set was generated from the pinned Node runtime's Object.getOwnPropertyNames(Object.prototype), not manually guessed. Native preserves this behavior; scheduler preserves Math.max NaN propagation and Node's NaN→1ms timeout coercion. Dedicated real scheduler test uses API `toString` and observes compaction only at1ms.

## Proof scope

`q15-idle-helper-green-1.log`: helper96 cases GREEN. `q15-idle-scheduler-green-4.log`: scheduler4 GREEN covering two provider deadlines/once-only, unsupported API/small context/settings/reference rejection, queued input/model change/disposal/manual compaction rejection and inherited API coercion. Scenarios sharing API names serialize access to the process-global fake provider registry; production ordering is unchanged. Earlier batch failure was reproduced as a cross-scenario registration collision; the same isolated case passed.

Remaining dedicated runtime scenarios: startup/reload restoration, observer, tool-busy deadline crossing and latest-request timer replacement. Final lane gates are not claimed by this slice.
