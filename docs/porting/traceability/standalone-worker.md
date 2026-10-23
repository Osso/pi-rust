# noSupervisor cross-module slice

Contract: [spec](../../specs/standalone-worker.md). Pin: [source-lock](../source-lock.json).

| Boundary | Current proof |
|---|---|
| SDK default memory manager | `standalone_worker_sdk` persisted-default RED→GREEN; rejects persisted manager before cwd/runtime IO and orchestrated options |
| Shared Partial-config validation and capability set | `differential_standalone_worker`: actual frozen42 partial-config cases and exact standalone-disabled insertion order |
| AgentSession field, tool activation, detach/inbound, approval, context | SDK behavior: prohibited tool activation blocked, detach registry absent, context flag true; actual faux write call denied without Supervisor contact or policy mutation |
| Advisory/goal-review direct context gates | SDK direct tool/goal calls reject with the frozen standalone reasons |
| Runtime resume/import/create guards | `standalone_worker_runtime`: resume/import denied before missing paths/factory work; persisted manager rejected before cwd resolution |
| Main optional coordination and exact startup gates | `Option<FirstPartyOrchestration>` matches frozen main.ts:637–641; source noSupervisor branch766–791. Existing main20 GREEN. `standalone_worker_main_process` drives real main CLI with an injected faux provider: noSupervisor true, memory-only session, no store/detach registry/runtime listener, successful print turn. `standalone_worker_factories` loads the no-orchestration first-party set and proves prohibited factory omissions while small extensions remain |

Factory omission follows frozen main.ts:665–685,704–727: no orchestration means no child/attached factories, agents-core/viewer/mailbox/status or goal; Pyrun uses the plain extension without the multi-agent request handler. Other small first-party extensions remain installed. Existing unrelated first-party inventory/order gaps are not reclassified by this slice.

This is not whole-module acceptance for AgentSession/main/SDK/runtime. Source identity-dependent restriction guards remain a separate approved integration slice.
