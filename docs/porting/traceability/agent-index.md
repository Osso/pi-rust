# Traceability: `packages/agent-core/src/index.ts`

Reference: immutable `4f9e6f8a4`. Rust: `crates/pi-agent-core/src/lib.rs` and `agent/mod.rs`.

| Frozen group | Rust | Status |
|---|---|---|
| agent.ts | Agent/options/queue-mode and supporting Rust structural types | [Agent mapping](agent.md); current targeted proof, reference gaps |
| agent-loop.ts | six public exports including restored_pending_tool_message | [loop mapping](agent-loop.md); current targeted/differential proof |
| proxy.ts | proxy event/options/stream | [proxy mapping](agent-proxy.md); targeted proof, explicit gaps |
| types.ts | types glob + explicit harness primitive re-exports | [types mapping](agent-types.md); shared representation gaps |
| harness/agent-harness.ts, compaction, messages, templates, sessions, skills, system prompt, harness/types, shell-output, truncate, uuid | existing harness/root modules | other slices; not revalidated here |

Removed core-only obsolete surfaces: public stream-fn module/get/set default registry, default-missing/reset-active error constructors, ScriptedAssistantStream, AgentOptions.stream_options bag, AgentToolResult usage/added_tool_names, AfterToolCallResult usage, BeforeToolCallResult terminate, stop-hook extra signal, proxy end-event tool_call payload.

Remaining old-only root exports require their owning slices: telemetry schemas/context helpers, search, harness tools, broad old session exports, compaction usage helpers and additional harness result types. Public implementation modules such as jsdiff/unicode are consumed by coding-agent and were not pruned here. Parent explicitly limited this slice's pruning to core APIs. Root row remains todo, not full index parity.

[Current proof ledger](agent-core-api-repin.md).
