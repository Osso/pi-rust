# agent-session-services.ts traceability

Rust: `crates/pi-coding-agent/src/core/agent_session_services.rs`.

| TS | Rust | Tests |
| --- | --- | --- |
| `createAgentSessionServices` | `create_agent_session_services` | `tests/sdk.rs::services_and_from_services_create_a_working_session` |
| `createAgentSessionFromServices` | `create_agent_session_from_services` | same |
| `AgentSessionServices` | `AgentSessionServices` | same |
| `AgentSessionRuntimeDiagnostic` | same | — |
| `applyExtensionFlagValues` | gap (extension runtime flags) | — |

## Gaps

- `DefaultResourceLoader` construction is not ported; `resource_loader` is required.
- Extension pending provider registrations / flag diagnostics are not applied.
