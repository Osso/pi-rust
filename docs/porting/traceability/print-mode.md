# print-mode traceability

Pinned source: `packages/coding-agent/src/modes/print-mode.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Proof: `tests/differential_print_mode.rs` (5 scenarios) drives the real pinned TypeScript
`runPrintMode` and the Rust port through `tests/bin/pi_mode_harness.rs` over stdout with the faux
provider, comparing stdout, stderr, and exit codes byte-for-byte after substituting only ids,
timestamps, durations, and temp paths.

| TypeScript source | Observable behavior | Rust mapping and proof |
|---|---|---|
| `PrintModeOptions` | `mode`, `messages`, `initialMessage`, `initialImages` | `PrintModeOptions` + `PrintMode` |
| `isPrintableAssistantMessage` | `error`/`aborted` stop reasons or any non-empty text block | `is_printable_assistant_message`; exercised by the text and error scenarios |
| `getEndTurnReason` | Last `end_turn` tool call with a non-blank `reason` | `get_end_turn_reason` (uses `pi_js_compat::string::js_trim`); end-turn-only scenario |
| `findLastPrintableAssistantMessage` | Reverse scan over state messages | `find_last_printable_assistant_message`; text scenario |
| `findLastEndTurnReason` | Reverse scan over assistant messages | `find_last_end_turn_reason`; end-turn-only scenario |
| `disposeRuntime` | Unsubscribe, dispose the runtime once | `PrintState::dispose` (idempotent flag) |
| `registerSignalHandlers` | `SIGTERM` → exit 143, `SIGHUP` → exit 129 after killing detached children and disposing | `register_signal_handlers`/`spawn_signal_handler`; process-level, not covered by an automated test (killing the harness would terminate the test runner's child) |
| `rebindSession` | Bind extensions with mode `print`/`json`, control DB path, command-context actions, `onError` to stderr; resubscribe session and agent listeners | `rebind_session`; differential scenarios emit `session_shutdown` through the real runtime |
| `mode === "json"` header | Session header line before binding | `write_json_line(header)`; JSON scenario |
| JSON event stream | Frozen toJsonPrintModeEvent shapes print JSON only | to_json_print_mode_event + write_json_line; common-pin module differential and scoped DoneReason process proof |
| Text result | Print every text block of the completed/last printable assistant message | `write_text_result`; text scenario |
| Error result | `errorMessage || "Request <stopReason>"` on stderr, exit 1 | `error_message_or_request`; error scenario |
| Empty result with `end_turn` reason | Print the reason | end-turn-only scenario |
| Prompt errors | Print `error.message` on stderr, exit 1 | `run_prompts` maps `AgentSessionError::message` |
| `finally` | Cleanup handlers, dispose, flush raw stdout | `run_print_mode` tail |

## Exclusions and deviations

- `process.stdout.write` is replaced by the ported raw-stdout queue (`core/output_guard.rs`).
- Signal handlers are process-global; the differential harness never sends them.
- The TypeScript `session.prompt` overload with an options object maps to `PromptOptions`.
- Test harness deviation: the Rust harness registers an `end_turn` tool definition with
  `label: undefined`-equivalent metadata (no `label` field in the Rust definition) and the TS
  harness overrides the built-in `end_turn` definition with the same field set, so the faux
  provider's context serialization (and therefore its token estimates) is identical. This only
  affects the harness, not the ported modules.
- Faux `usage` token counts are byte-identical in the differential; tool execution event times
  differ only in the approved `serde_json` integral-`f64` spelling (`…107.0` vs `…107`), which the
  jsonl serializer discrepancy already classifies.
- No source bug found.
