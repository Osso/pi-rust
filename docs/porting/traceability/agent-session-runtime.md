# agent-session-runtime.ts traceability

Rust: `crates/pi-coding-agent/src/core/agent_session_runtime.rs`.

| TS | Rust | Tests |
| --- | --- | --- |
| `AgentSessionRuntime` | `AgentSessionRuntime` | compile-level; runtime flow not covered by tests yet |
| `createAgentSessionRuntime` | `create_agent_session_runtime` | — |
| `CreateAgentSessionRuntimeFactory` | `CreateAgentSessionRuntimeFactory` | — |
| `switchSession`, `newSession`, `fork`, `relocate`, `restart`, `importFromJsonl`, `dispose` | same names | — |
| `prepare/activateToolResultRelocation` | same | — |
| `CreateAgentSessionRuntimeResult` | same | — |

## Gaps

- `session_before_switch` / `session_before_fork` extension hooks and `emitSessionShutdownEvent`: no behavioral emission yet.
- Process restart (`restartCurrentProcess`, `persistForRecovery` handoff) is an injected `ProcessRestarter`; default returns an error.
- `runDetachedJobArtifactCleanup`: detached-job slice.
- `withSession` receives a bare `AgentSession` instead of `ReplacedSessionContext` (extension context slice).
