# extensions/multi-agent.ts

Whole21-line barrel audited against frozen [4f9e6f8a4](../source-lock.json) on2026-10-25. It contains only reexports; there are no runtime branches. Native API layout follows the approved Rust-native extension contract, not a TypeScript loader shim.

| Frozen exports | Existing public native location |
|---|---|
| `registerAgentViewerTools` | `bundled_extensions::agents_core::runtime::register_agent_viewer_tools` |
| `createMultiAgentRuntimeHandles`, `deliverTerminalOutboxForStore`, `registerAgentsMailboxTools`, `resolveMultiAgentStore`, default | `agents_core::runtime::{create_multi_agent_runtime_handles,deliver_terminal_outbox_for_store,register_agents_mailbox_tools,resolve_multi_agent_store,multi_agent_extension}` |
| `createProductionAttachedSessionFactory`, `createProductionChildAgentSessionFactory` | `agents_core::session_factories::{create_production_attached_session_factory,create_production_child_agent_session_factory}` |
| `registerAgentsCoreTools` | `agents_core::registration::register_agents_core_tools` |
| `AgentDesktopNotification`, `AgentDesktopNotifier` | `core::desktop_notification::{DesktopNotification,DesktopNotifier}` |
| `AttachedSessionDispatchInput`, `AttachedSessionFactory`, `ChildAgentDispatchInput`, `ChildAgentSessionFactory` | same snake-case module/type equivalents in `agents_core::shared` |
| `MultiAgentExtensionOptions`, `MultiAgentRuntimeHandles` | same types in `agents_core::runtime` |

All9 runtime values and8 types have native counterparts; no old-only behavior or new compatibility API was added. Existing native consumers compile these public paths. This closes the barrel only, not [agents-core runtime acceptance](agents-core.md). [Terminal projection proof](terminal-outbox-delivery.md) remains independently scoped.
