# Agent viewer — frozen 4f9e6f8a4

Sources `extensions/agent-viewer/src/{index,runtime}.ts`; native `agent_viewer/mod.rs`, `agents_core/runtime.rs::register_agent_viewer_tools`, `agents_core/viewer.rs`.

| Symbols / branches | Proof |
| --- | --- |
| Default factory, `registerAgentViewerTools`, schema and registration | Exact frozen full-tool metadata differential (12 tools); native default factory forwards options to the same registration helper |
| `agentViewer`: trace requires nonempty storeSessionId; memory/not-found; empty/valid/mismatched session selectors | Deterministic frozen cases; empty session/store selectors reproduced native mismatch and now follow JS truthiness |
| `agentViewerFromPersistedSession`, `loadPersistedAgentsForViewer`: absent/empty DB, missing metadata, empty state, agent lookup, transcript validation, optional trace read | Shared viewer behavioral tests and seven real same-SQLite/files frozen historical trace cases; missing context DB is also differential |
| `isPersistedAgentSnapshot`: id/lifecycle/revision shape gate | Same native predicate; malformed exotic agent payloads follow the project boundary, not an expanded typed API |
| `agentViewerResult`, `formatAgentViewerContent`: children, parent, active/terminal summary/error/code, status, optional trace/transcript | Deterministic source/native exact text/details, plus [historical trace proof](agent-lifecycle-trace.md); lossless JSON serialization failures now propagate rather than replacing agent with null |
| `validateTranscriptSession`, missingControlDb/persistedSessionNotFound/persistedAgentNotFound | Same messages/detail fields; memory mismatch/not-found/trace guards are differential; persisted branches are behavioral |
| `viewStatus`, `viewTranscript`, `listViewerCommands`, result/errorResult | Exact deterministic parsed outputs (including no isError), same fixed stop/steer commands for active and terminal agents |

Proof: `cargo test -j3 -p pi-coding-agent --test suite_core_1 differential_agents_core_deterministic::` RED `/tmp/agents-core-viewer-empty-red.log` → GREEN `/tmp/agents-core-viewer-empty-green.log` (both tests pass). Existing core lifecycle trace parser/formatter coverage belongs to its own module; viewer dispatch does not duplicate that algorithm.

Gap: pathological input shapes, numeric extremes and OS/runtime fault shapes follow the project parity boundary.
