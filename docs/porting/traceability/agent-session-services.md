# agent-session-services.ts traceability

Frozen: `4f9e6f8a4`, `packages/coding-agent/src/core/agent-session-services.ts` (218 lines), audited whole module 2026-10-25.
Rust: `crates/pi-coding-agent-core/src/core/agent_session_services.rs`.

| Frozen exports / branches | Rust | Proof |
| --- | --- | --- |
| `AgentSessionRuntimeDiagnostic`, service and creation-option interfaces | matching structs; diagnostic `type` serialized explicitly | `agent_session_services`, `sdk`, main diagnostics serialization |
| `createAgentSessionServices`: cwd resolution; truthy agentDir/default; auth, settings then model creation | `create_agent_session_services` | `services_paths_and_flag_diagnostics_match_frozen` compares actual frozen results for empty, absent, explicit agentDir |
| Default loader option spread overridden by cwd/agentDir/settings; reload options | `build_resource_loader`, `reload_with_options` | `default_resource_loader_options_build_a_loader`; existing resource-loader reload tests |
| Pending provider registration, per-registration caught diagnostic, clear after loop | `register_pending_providers` | existing resource-loader/extension registration tests; no dedicated differential registration fixture |
| `applyExtensionFlagValues`: absent map; last registered flag wins; boolean presence always true; string and missing-string values; ordered plural unknown diagnostics | `apply_extension_flag_values` | 3 prior behavioral tests; frozen differential includes false boolean, missing string value, two unknown flags and exact diagnostic order |
| `createAgentSessionFromServices`: all 23 forwarded fields including noSupervisor, noTools, excludeTools, permissionPromptTool, runtime role/capability and sessionStartEvent | `create_agent_session_from_services` | `sdk::services_and_from_services_create_a_working_session`; literal field-by-field audit |

## Re-pin fixes

Empty `agentDir` now selects `getAgentDir()` rather than resolving the empty path. Actual-frozen RED: `/tmp/repin-agent-session-services-red3.log` showed the private default agent dir versus process cwd mismatch. Settings are created before model registry, matching frozen initialization order.

## Rust-only seams and gaps

The existing injected `resource_loader`, `base_tools` and shared-channel delivery callback remain SDK/embedding seams; normal unset behavior follows frozen. No new API or dependency was added. Adversarial inputs, OS-fault shapes and Node runtime oddities are excluded by the repository parity bar. UTF-16 flag names/values remain governed by the existing extension/CLI carriers; this module adds no scalar projection.
