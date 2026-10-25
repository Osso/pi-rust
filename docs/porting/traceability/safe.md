# safe traceability

Source: `packages/coding-agent/extensions/safe/src/index.ts` (71 lines).
Rust: `crates/pi-coding-agent-extensions/src/bundled_extensions/safe.rs`.
Tests: `crates/pi-coding-agent/tests/bundled_extensions_gates.rs` (`safe_matches_typescript`).

| Source symbol / branch | Rust item | Proof |
| --- | --- | --- |
| `safeCommandActions` / `parseSafeCommand` (`args || "status"`) | `SAFE_COMMAND_ACTIONS`, `parse_safe_command` | differential status/bogus steps |
| `safeModeStatus` | `safe_mode_status` | differential notify text |
| `updateSafeStatus` (undefined clears) | `update_safe_status` | differential setStatus calls |
| `allowedSafeModeTools` gate | `is_allowed_safe_mode_tool`, `block_disallowed_tool` | differential six gate steps |
| `getArgumentCompletions` (empty -> null) | `register_command` completions | differential prefixes `o`, `s`, `x` |
| `session_start` status refresh | handler | registration + handler |
| command metadata/description | `register_command` | registration differential |

No gaps.
