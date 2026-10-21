# agent-model-tool.ts

Frozen source: `4f9e6f8a4c0065d81b10e7177b28d5082f0bc30d`, `packages/coding-agent/extensions/agents-core/src/agent-model-tool.ts` (whole module).

Rust: `bundled_extensions/agents_core/agent_model_tool.rs`; exact registration in `registration.rs`, child capability suppression in `core/tool_capabilities.rs`; source-implied optional-effort mutation method in `ViewedSessionMutationTarget` and `AgentSessionMutationTarget`; direct live setters/publication in `core/agent_session.rs`.

| Source symbols/branches | Behavioral evidence | Level |
|---|---|---|
| `registerSetAgentModelTool`, complete schema and registration metadata | `set_agent_model::registers_agent_model_tool_without_approval` compares real frozen factory metadata | TS differential |
| `readRequestedModel`: absent/blank pair, half pair, at least one change; validation before lookup | `errors_and_order_match_frozen_registered_tool` | TS differential |
| `resolveLiveAgentMutationTarget`: missing, background-before-inactive, inactive, missing live session | same error differential | TS differential |
| `hasMutationSurface`: no mutation target | pending focused fixture | Pending |
| `setAgentModel`: model+effort, JS whitespace, clamped effective result, selected-agent preservation | `live_request_keeps_old_model_next_request_uses_effective_model_and_effort` | Native live AgentSession + faux provider |
| Current request unchanged, no cancellation; next request receives new model/effective effort | same held-request test records provider inputs | Native runtime boundary |
| Source direct setters publish effective metadata; child settings unchanged | same test observes store and global settings | Native in-memory |
| Effort-only, model-only, unknown model, all effort literals, schema admission | pending focused differential | Pending |
| Optional model result properties | conditional Rust output follows source; no runtime fixture yet | Pending |
| Ownership/revision rejection and TS-created state → Rust mutation → same TS read; reciprocal fresh Rust state → TS read | pending dedicated fixtures | Pending |

Three targeted tests pass at initial implementation. This document does not promote broad agents-core or central coverage. Governing contract: [spec](../../specs/set-agent-model.md).
