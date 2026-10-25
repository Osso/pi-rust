# Detached runtime cancellation — 4f9e6f8a4

Frozen: `extensions/agents-core/src/detached-runtime-cancellation.ts` and `runtime.ts:2551–2620`. Native: `agents_core/control_runtime.rs`.

| Symbol / branch | Native | Proof / remaining work |
| --- | --- | --- |
| `requestPersistedDetachedRuntimeCancellation`: persistence/output label/ownership/coordinator checks, rejected mutation, publish snapshot | `request_persisted_detached_runtime_cancellation` | Coordinator cancellation tests and memory ownership-unavailable differential; tree/terminal-cleanup tests use real persisted ownership/mailbox. Original output-label predicate no longer wrongly requires a worker for this API |
| `isDetachedRuntimeAgent`: detached + runtime worker + background output label | `is_detached_runtime_agent` | `frozen_settlement_abort_and_detached_predicate_match`: seven actual frozen cases cover detached=false, terminal worker, wrong type/label, missing result, Bash and Pyrun output |
| Authoritative snapshots: memory without persistence, DB otherwise; missing state throws | `read_authoritative_agent_snapshots` | Tree test uses real SQLite; five actual frozen memory cases prove candidate/active/direct-parent filtering and exact unavailable-ownership cleanup failure text. Missing persisted state retains the explicit source error |
| `readDetachedRuntimeAgentIds`: candidate + active + detached classification | `read_detached_runtime_agent_ids` | `lifecycle_coordinator::cancelling_tree_suppresses_detached_descendants_but_not_root` |
| Direct children cancellation: active direct detached runtimes only; reason and suppression true; explicit cleanup rejection | `request_direct_detached_runtime_cancellations` | `agents_core_runtime::child_terminal_path_cancels_detached_runtime_before_settling`: actual spawn/prompt creates detached child after parent admission; parent remains running until child acknowledges cancellation. RED timed out waiting for cancellation; GREEN checks persisted suppression and parent completion after acknowledgement |
| Runtime reversed descendant cancellation, root suppression absent | `cancel_owned_agent_runtime` | Behavioral RED (`/tmp/agents-core-cancel-red3.log`: child flag missing) → GREEN (`/tmp/agents-core-cancel-green2.log`); real coordinator, ownership, SQLite projection and two mailbox commands |

Additional proof: `agents_core_runtime::detached_memory_selection_and_cleanup_failures_match_frozen` invokes actual frozen selection/direct-cleanup helpers in five memory cases, including terminal/non-runtime/wrong-parent filtering and the complete cleanup error. `/tmp/agents-core-settlement-closure-green.log`: all 26 runtime tests pass.

Gap: adversarial JSON shapes and OS/runtime fault shapes follow the project parity boundary.
