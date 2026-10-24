# approval-controls traceability

Source: `packages/coding-agent/extensions/approval-controls/src/index.ts` (60 lines).
Rust: `crates/pi-coding-agent-extensions/src/bundled_extensions/approval_controls.rs`.
Tests: `crates/pi-coding-agent/tests/bundled_extensions_gates.rs` (`approval_controls_matches_typescript`).

| Source symbol / branch | Rust item | Proof |
| --- | --- | --- |
| `SANDBOX_USAGE` | `SANDBOX_USAGE` | differential usage steps |
| `parseSandboxSelection` (lowercase, 2 args, inherit/session only) | `parse_sandbox_selection` | differential seven argument cases |
| `clearEditor` | `clear_editor` | differential setEditorText calls |
| `handleSandboxCommand` (selector, invalid, set, notify) | `handle_sandbox_command` | differential |
| `/approvals` selector | command handler | differential |
| `/sandbox` command metadata | `register_command` | registration differential |
| `ctx.setSandboxProfile` | `ExtensionCommandContext::set_sandbox_profile` | injected command action; call recorded |

Gaps:
- `ExtensionCommandContext::set_sandbox_profile` delegates to
  `ExtensionCommandContextActions::set_sandbox_profile`. Print/RPC modes bind the real
  implementation (`AgentSession::set_sandbox_profile`). The interactive mode must bind
  the same action in the ca-interactive-mode slice; until then `/sandbox <profile>
  <scope>` in interactive mode fails with `Sandbox profile settings are unavailable`.
- `SessionManager::set_session_sandbox_profile` is a new faithful port used by
  `AgentSession::set_sandbox_profile` (persist recovery, metadata snapshot,
  write/clear control-db profile).
