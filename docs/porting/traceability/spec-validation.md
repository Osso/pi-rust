# spec-validation traceability

Source: `packages/coding-agent/extensions/spec-validation/src/index.ts` (34 lines).
Rust: `crates/pi-coding-agent/src/bundled_extensions/spec_validation.rs`.
Tests: `crates/pi-coding-agent/tests/bundled_extensions_commands.rs` (`spec_validation_matches_typescript`).

| Source symbol / branch | Rust item | Proof |
| --- | --- | --- |
| `SPEC_VALIDATION_PROMPT` | `SPEC_VALIDATION_PROMPT` | differential sendUserMessage text |
| idle gate (`/spec-validation is blocked...`) | handler `is_idle` branch | differential error step |
| `sendUserMessage(prompt)` + editor clear | handler | differential calls |
| command metadata/description | `register_command` | registration differential |

No gaps.
