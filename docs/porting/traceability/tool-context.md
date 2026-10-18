# Execution tool context traceability

Pinned source: `packages/agent/src/harness/tools/tool-context.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Rust: `crates/pi-agent-core/src/harness/tools/tool_context.rs`.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `tool-context.ts:3-6` | `ExecutionToolContext` carries the `ExecutionEnv` used by every built-in tool | `ExecutionToolContext { env: Arc<dyn ExecutionEnv>, file_mutation_queue: Arc<FileMutationQueue> }`; all tool tests construct it via `ExecutionToolContext::new` |

## Rust boundary

`AgentTool::execute` has no per-call context parameter, and the harness orchestration layer that supplies `TContext` is not yet ported. Tools therefore bind `Arc<ExecutionToolContext>` at construction. The extra `file_mutation_queue` field replaces the source's per-environment `WeakMap` so write/edit share serialization without a global registry.
