# Historical agent lifecycle trace

Whole frozen `core/agent-lifecycle-trace.ts` at `4f9e6f8a4` maps to `core/agent_lifecycle_trace.rs`; the agent-viewer persisted path uses the trace only with explicit storeSessionId.

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

Reader never mutates DB/transcripts. Ownership JSON uses the shared JavaScript parser, not serde parsing, and number interpolation uses JavaScript Number-to-string semantics. Implemented but additional proof pending: malformed ownership JSON/shape, missing child transcript, mismatched child header, cycles/duplicate descendants, claimed/poisoned outbox formatting. These are not waived.
