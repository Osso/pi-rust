# agents-mailbox traceability

Source: `packages/coding-agent/extensions/agents-mailbox/src/index.ts` (9 lines).
Rust: `crates/pi-coding-agent-extensions/src/bundled_extensions/agents_mailbox/mod.rs`.

| Source symbol | Rust item | Proof |
| --- | --- | --- |
| default `agentsMailboxExtension(pi, options)` | `agents_mailbox_extension` | registration metadata differential (`send_agent_message`, `contact_parent`) |
| delegation to `registerAgentsMailboxTools` | `agents_core::runtime::register_agents_mailbox_tools` | same |
| `onSessionMessageSent` option pass-through | `MultiAgentExtensionOptions::on_session_message_sent` | mailbox delivery test (callback path compiled; notifier callback not asserted) |

Mailbox transport, error text, and runtime rows are covered in
`docs/porting/traceability/agents-core.md` (`mailbox-runtime.ts`).

Gaps: none specific to this factory.
