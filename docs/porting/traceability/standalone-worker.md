# noSupervisor cross-module slice

Contract: [spec](../../specs/standalone-worker.md). Pin: [source-lock](../source-lock.json).

| Boundary | Current proof |
|---|---|
| SDK default memory manager | `standalone_worker_sdk` reproduced native persisted default RED; GREEN pending integration |
| Shared Partial-config validation; AgentSession field, tools, detach, inbound, approval, context | Implementation in progress; targeted gate pending |
| Advisory/goal-review direct context gates | Implemented; acceptance pending |
| Runtime resume/import/create guards | Pending |
| Main optional coordination and exact startup gates | Pending |

This is not whole-module acceptance for AgentSession/main/SDK/runtime. Source identity-dependent restriction guards remain a separate approved integration slice.
