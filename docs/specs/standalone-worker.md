# Standalone worker

Frozen `noSupervisor` paths across AgentSession, SDK, runtime, main and extension/tool boundaries must match the [common pin](../porting/source-lock.json).

- Reject persisted managers and multi-agent orchestration before constructing runtime services; default standalone SDK managers are in-memory.
- Keep standalone-disabled tools unavailable at registry/activation, before-tool and nested-call boundaries. Preserve end_turn. Advisory and goal-review tools reject direct standalone context calls.
- Disable detachment and inbound runtime coordination. Expose noSupervisor on session/extension context. LLM-dependent approval blocks without contacting Supervisor or changing approval policy; source read-only auto-approval remains intact.
- Runtime resume/import reject standalone mode before side effects. Runtime creation rejects persisted standalone managers and skips detached cleanup/archive.
- CLI no-supervisor disables first-party coordination and all frozen startup/runtime Supervisor branches; no alternate facade or hidden store remains.

Restriction recovery's supervised-main-session gate consumes this field in the subsequent integration slice.
