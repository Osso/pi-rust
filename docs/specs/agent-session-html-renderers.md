# Agent-session HTML tool renderers

Frozen authority: `agent-session.ts:5457–5505,6322–6342` at `4f9e6f8a4`.

- [x] Export uses the original registered definition for each tool, including its custom call and result renderers.
- [x] Custom result output is included in both collapsed and expanded export views.
- [x] Registry replacement order and allowed/excluded tool filtering remain unchanged.

The private registry retains the existing extension `ToolDefinition` alongside its wrapped executable projection. No new carrier type, public API or dependency is introduced. The metadata-only export reconstruction, which discarded renderers, is removed.

Proof: `agent_session::html_renderers` executes a custom tool and checks the actual exported payload for its call, collapsed-result and expanded-result HTML. The test failed with a missing rendered-tool entry and passes after retaining the original definition. Session JSONL serialization is unchanged. See the [proof ledger](../porting/agent-session-repin-proof.md).
