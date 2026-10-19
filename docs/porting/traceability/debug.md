# debug traceability

Source: `packages/coding-agent/extensions/debug/src/index.ts` (29 lines).
Rust: `crates/pi-coding-agent/src/bundled_extensions/debug.rs`.
Tests: `crates/pi-coding-agent/tests/bundled_extensions_commands.rs` (`debug_matches_typescript`).

| Source symbol / branch | Rust item | Proof |
| --- | --- | --- |
| `DebugReplController` interface | `DebugReplController` trait | injected recorder |
| `getController` factory argument | `DebugReplControllerProvider` | injected provider |
| `off` branch + notify | command handler | differential |
| non-empty action usage warning | command handler | differential |
| enable + `pi debug attach <id>` warning | command handler | differential |
| command metadata/description | `register_command` | registration differential |

Gaps:
- `core/debug-repl.ts` remains the documented `node:vm` gap. The extension's controller
  provider must be supplied by the caller; no REPL server exists in Rust.
