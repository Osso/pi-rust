# Historical agent lifecycle trace

Whole frozen `core/agent-lifecycle-trace.ts` at `4f9e6f8a4` maps to `core/agent_lifecycle_trace.rs`; the agent-viewer persisted path uses the trace only with explicit storeSessionId. Whole-module audit completed 2026-10-25 under the lighter-proof rule.

| Export / branches | Native | Proof |
|---|---|---|
| readAgentLifecycleTrace | read_agent_lifecycle_trace | differential_agent_lifecycle_trace reads same isolated SQLite and JSONL as frozen |
| agentSnapshotEvent / descendantTraceEvents / listTraceDescendants | snapshot + BFS adjacency/visited set | root, child, grandchild and unrelated agent fixtures |
| readRuntimeOwnership / parseProcessIdentity | readonly ownership query + parsed process value | owner main, session, pid/ticks/incarnation differential; 100.0 / -0 / 1e21 formatting RED reproduced JSON-number drift; shared JS parser/number formatter fixes it; float/-0/exponent/absent ownership cases; overflow pid/ticks fixtures reproduce nonfinite loss and now retain binary64 until formatting, serializing nonfinite fields as null (GREEN; finite-only mutant killed) |
| readTerminalOutboxEvents | per-root/descendant ordered rows | pending child event, attempts/error fields differential |
| readParentAgentEvents / parentAgentEvent | real session-manager file entries | matching start/complete, child transcript reference |
| readChildEndTurnEvents / childEndTurnEvent / readEndTurnReason | transcript ID/header/timestamp filter; successful end_turn only | successful and errored child end_turn fixtures |
| compareTraceEvents / TRACE_EVENT_ORDER | stable locale timestamp sort + kind priority | all events share a timestamp, exact ordered trace differential |
| formatAgentLifecycleTrace / formatOwnership / formatTraceEvent | same formatting helpers | exact viewer text plus JSON differential |

Reader never mutates DB/transcripts. Ownership JSON uses the shared JavaScript parser, not serde parsing, and number interpolation uses JavaScript Number-to-string semantics.

## Successor audit

All 402 frozen lines audited: readonly DB lifecycle, parent adjacency/BFS and stable ordering, agent snapshots, optional runtime ownership/process fields, ordered outbox mapping, parent custom records and lifecycle filtering, child transcript presence/header/session/timestamp filters, successful end_turn extraction, timestamp locale ordering plus kind priority, and every formatter branch. Missing transcripts use the shared session reader; mismatched headers produce no child events. Claimed/poisoned outbox statuses use the same direct fields and formatter as pending. No new discrepancy found on realistic inputs; no code change or new proof matrix was needed.

Existing common-pin proof re-run: `cargo test -j3 -p pi-coding-agent --test suite_core_1 differential_agent_lifecycle_trace::` PASS, `/tmp/runtime-successor-lifecycle-trace.log`. The single test exercises seven same-DB/JSONL frozen viewer cases with exact JSON and displayed text; prior ownership number fixes remain covered. This module reads persistence through the shared already-proved codecs and writes none.

- GAP: malformed ownership/transcript shapes, cycles/duplicate adversarial descendants, numeric extremes and exotic OS/runtime faults use the approved input/runtime gap; exact UTF-16 carriers are retained.
No additional source bug was found. The output is a historical event list, not a reconciled lifecycle timeline.
