# agent-viewer traceability

Source: `packages/coding-agent/extensions/agent-viewer/src/index.ts` (9 lines).
Rust: `crates/pi-coding-agent-extensions/src/bundled_extensions/agent_viewer/mod.rs`.

| Source symbol | Rust item | Proof |
| --- | --- | --- |
| default `agentViewerExtension(pi, options)` | `agent_viewer_extension` | registration metadata differential (`agent_viewer` tool) + `agents_core_runtime` registration test |
| delegation to `registerAgentViewerTools` | `agents_core::runtime::register_agent_viewer_tools` | same |
| re-exported `MultiAgentExtensionOptions` | `agents_core::runtime::MultiAgentExtensionOptions` | same |

Frozen runtime.ts trace integration and whole core lifecycle trace now have same-DB/files text+JSON differential proof; see [lifecycle trace](agent-lifecycle-trace.md). Old viewer.ts proof below is historical, not complete new-pin coverage.

Gaps: none specific to this factory. It depends on the agents-core
`resolveSelectedSessionMutationTarget` gap only through unrelated interactive
wiring.
