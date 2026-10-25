# Detached runtime cancellation — 4f9e6f8a4

Frozen: `extensions/agents-core/src/detached-runtime-cancellation.ts` and `runtime.ts:2551–2620`. Native: `agents_core/control_runtime.rs`.

| Symbol / branch | Native | Proof / remaining work |
| --- | --- | --- |
| `requestPersistedDetachedRuntimeCancellation`: persistence/output label/ownership/coordinator checks, rejected mutation, publish snapshot | `request_persisted_detached_runtime_cancellation` | Existing coordinator cancellation tests; module-level error-branch differential pending |
| `isDetachedRuntimeAgent`: detached + runtime worker + background output label | `is_detached_runtime_agent` | Whole predicate ported; differential matrix pending |
| Authoritative snapshots: memory without persistence, DB otherwise; missing state throws | `read_authoritative_agent_snapshots` | Tree test uses real SQLite; memory/missing-state proof pending |
| `readDetachedRuntimeAgentIds`: candidate + active + detached classification | `read_detached_runtime_agent_ids` | `lifecycle_coordinator::cancelling_tree_suppresses_detached_descendants_but_not_root` |
| Direct children cancellation: active direct detached runtimes only; reason and suppression true; explicit cleanup rejection | `request_direct_detached_runtime_cancellations` | Runtime terminal-cleanup wiring and focused proof pending |
| Runtime reversed descendant cancellation, root suppression absent | `cancel_owned_agent_runtime` | Behavioral RED (`/tmp/agents-core-cancel-red3.log`: child flag missing) → GREEN (`/tmp/agents-core-cancel-green2.log`); real coordinator, ownership, SQLite projection and two mailbox commands |

Gap: adversarial JSON shapes and OS/runtime fault shapes follow the project parity boundary.
