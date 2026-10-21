# agent-model-tool.ts

Frozen source: `4f9e6f8a4c0065d81b10e7177b28d5082f0bc30d`, `packages/coding-agent/extensions/agents-core/src/agent-model-tool.ts` (whole module).

Rust: `bundled_extensions/agents_core/agent_model_tool.rs`; exact registration in `registration.rs`, child capability suppression in `core/tool_capabilities.rs`; source-implied optional-effort mutation method in `ViewedSessionMutationTarget` and `AgentSessionMutationTarget`; direct live setters/publication in `core/agent_session.rs`.

| Source symbols/branches | Behavioral evidence | Level |
|---|---|---|
| `registerSetAgentModelTool`, complete schema and registration metadata | `set_agent_model::registers_agent_model_tool_without_approval` compares real frozen factory metadata | TS differential |
| `readRequestedModel`: absent/blank pair, half pair, at least one change; validation before lookup | `errors_and_order_match_frozen_registered_tool` | TS differential |
| `resolveLiveAgentMutationTarget`: missing, background-before-inactive, inactive, missing live session | same error differential | TS differential |
| `hasMutationSurface`: no mutation target; optional absent model | `missing_mutation_surface_and_absent_model_properties_match_source`; remaining structural properties are required by Rust trait | TS differential |
| `setAgentModel`: model+effort, JS whitespace, clamped effective result, selected-agent preservation | `live_request_keeps_old_model_next_request_uses_effective_model_and_effort` | Native live AgentSession + faux provider |
| Current request unchanged, no cancellation; next request receives new model/effective effort | same held-request test records provider inputs | Native runtime boundary |
| Existing parent `set_model` delegates without losing default writes; selected-view mutation remains independent | `parent_default_set_model_path_retains_model_and_effort_defaults`; five existing `agent_session_mutation_target` tests | Native targeted regression |
| Source direct setters publish effective metadata; child settings unchanged | same test observes every store publication (not only final state), captured request cancellation token in owning test thread, and global settings | Native runtime boundary |
| Effort-only, model-only, model+effort, unknown model, all effort literals and nonreasoning clamp | `live_effective_results_and_effort_literals_match_frozen_session` (13 cases) uses source factory with real source AgentSession setters | TS differential |
| Optional provider/model result properties and `SetAgentModelDetails` | `missing_mutation_surface_and_absent_model_properties_match_source` compares omitted properties and no-model text | TS differential |
| Ownership rejection and lifecycle revision preserved | `mismatched_persisted_owner_rejects_model_and_effort_before_live_mutation`; real source-created process-owned state, unchanged live model/effort and same TS reader | Cross-runtime SQLite boundary |
| TS-created state → Rust registered tool mutation → same TS reader; reciprocal fresh Rust state → same TS reader | `source_owned_state_and_fresh_native_state_are_readable_by_same_source_after_tool_mutation` covers both directions, model+effort and effort-only | Cross-runtime SQLite boundary |
| Schema runtime argument admission (14 cases) | `schema_argument_admission_matches_source_validation`: 12 agree; optional null provider/model and null effort disagree in existing shared `pi-ai` validator | BLOCKED shared validator alignment |

Five focused behavioral mutants killed: ignore explicit effort; resolve selected instead of requested agent; skip pre-mutation publication; cancel held request; publish requested instead of effective effort. The first cancellation mutant survived because an assertion inside a faux task was swallowed; the assertion now observes the captured token in the owning test thread and kills it. Source/native fixture manifests record frozen source SHA-256, source revision, schema version 15, captured time, initial-state hash and both post-mutation observations.

Current targeted evidence: eight contract tests pass across focused runs, one shared-validator differential fails; five existing selected-view regression tests pass. The pre-existing `agent_session` integration target cannot compile its unrelated mailbox fixtures (`recipient_host` missing and retired `RuntimeMailboxMessageStatus::Delivered`); no edits made there. Frozen source validator retains/coerces optional null strings and rejects null effort; existing Rust validator strips optional nulls. No workaround or waiver applied. This document does not promote broad agents-core or central coverage. Governing contract: [spec](../../specs/set-agent-model.md).
