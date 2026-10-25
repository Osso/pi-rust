# session-id traceability

Source: `packages/coding-agent/extensions/session-id/src/index.ts` (10 lines).
Rust: `crates/pi-coding-agent-extensions/src/bundled_extensions/session_id.rs`.
Tests: `crates/pi-coding-agent/tests/bundled_extensions_commands.rs` (`session_id_matches_typescript`).

| Source symbol / branch | Rust item | Proof |
| --- | --- | --- |
| `/session-id` command | `register_command` | registration differential |
| `Session ID: <id>` notification | handler | differential notify call |

No gaps.
