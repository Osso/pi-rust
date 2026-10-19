# self-restart traceability

Source: `packages/coding-agent/extensions/self-restart/src/index.ts` (32 lines).
Rust: `crates/pi-coding-agent/src/bundled_extensions/self_restart.rs`.
Tests: `crates/pi-coding-agent/tests/bundled_extensions_commands.rs` (`self_restart_matches_typescript`).

| Source symbol / branch | Rust item | Proof |
| --- | --- | --- |
| `RESTART_NOTICE` | `RESTART_NOTICE` | differential restart options |
| `/restart` command | `register_command` handler | differential restart call |
| `restart_self` tool metadata/schema (`Type.Object({})`) | `ToolDefinition` | registration differential |
| promptGuidelines (2 lines) | literal | registration differential |
| `execute` restart + result text/details | tool execute | differential tool result |

No gaps.
