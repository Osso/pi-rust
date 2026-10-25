# claude-memory-enrich traceability

Source: `packages/coding-agent/extensions/claude-memory-enrich/src/index.ts` (162 lines).
Rust: `crates/pi-coding-agent-extensions/src/bundled_extensions/claude_memory_enrich.rs`.
Tests: `crates/pi-coding-agent/tests/bundled_extensions_context.rs` (fake `PI_CLAUDE_MEMORY` scripts).

| Source symbol / branch | Rust item | Proof |
| --- | --- | --- |
| `isHookOutput` | `is_hook_output` | differential invalid-shape step |
| `resolveClaudeMemoryCommand` (env then default path) | `resolve_claude_memory_command` | differential env steps |
| `parseAdditionalContext` (trim/JSON.parse/shape/trim context) | `parse_additional_context` | differential ok/empty/invalid steps |
| `runEnrich` spawn, stdin payload, stdout/stderr, timeout, exit code | `run_enrich` | differential ok/failing steps; timeout branch implemented |
| `appendEnrichment` (skip existing section) | `append_enrichment` | differential step 6 |
| `before_agent_start` empty prompt / missing command | handler early returns | differential empty-prompt step |
| error logging (`console.error`) | `eprintln!` | observed in test output |

Gaps / discrepancies:
- Timeout kills with SIGKILL (source sends SIGTERM) after the same 15000 ms bound.
- Abort maps to Node's `The operation was aborted`; source spawn receives the
  AbortSignal directly.
- `JSON.parse` errors use `utils::js_json::parse` (V8 text); no differential case
  asserts the message.
- Handler errors are logged to stderr; the oracle cannot capture that stream, so
  tests compare the returned `null` behavior.
