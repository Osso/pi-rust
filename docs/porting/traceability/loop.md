# Loop extension traceability

Source: `packages/coding-agent/extensions/loop/src/index.ts`, common pin `4f9e6f8a4`. Rust: `crates/pi-coding-agent-extensions/src/bundled_extensions/loop/mod.rs`.

| Frozen symbol / branch | Rust | Proof |
|---|---|---|
| default command/tool metadata/schema, validation and notifications | `loop_extension`, `loop_tool_definition`, `execute_loop_tool`, `handle_loop_command` | existing `differential_loop_handlers` (14 cases), `bundled_commands::loop_*` |
| interval parse/default units/floor/minimum, format h/m/s/ms, command aliases | `parse_interval_ms`, `format_interval`, `parse_loop_command` | existing metadata/command differential |
| start stops previous timer, generation increments, captures session id; start after shutdown throws | `LoopController::start`, `shutdown` | `suite_extensions::loop_runtime::loop_runtime_matches_frozen_deferral_generation_and_session_guards` replacement/shutdown cases |
| stop/status, timer lifecycle | `stop`, `status`, `spawn_interval` | behavioral timer tests plus runtime differential |
| interval current generation/session guard; ignore in-flight | `handle_interval` | runtime differential: repeated ticks, replaced loop, changed/restored session |
| busy or pending interval defers rather than injecting | `handle_interval` | runtime differential: busy/pending cases |
| agent-end continuation returns; current session and matching generation message required to clear in-flight | registered hook, `handle_agent_end`, `includes_loop_prompt` | runtime differential: relocation, stale-generation message and completion cases |
| deferred agent-end checks pending/in-flight, not idle | `handle_agent_end` | runtime differential: busy-end and pending-end cases |
| custom loop message with generation/session details; visible, followUp, triggerTurn | `prepare_loop_prompt`, `send_loop_prompt` | full frozen message/options equality; existing native timer assertions updated to the frozen output contract |
| raw UTF-16 prompt in custom messages | native JsString/JsValue | raw-unit runtime differential; existing unit test |

The frozen runtime oracle calls the real default extension with controlled timers and explicit user-observable lifecycle events, without rewriting source. Native tests advance paused Tokio timers and use actual extension contexts/session managers. Eight scenarios compare output after every operation and all emitted messages/options.

## Proof ledger

- RED `/tmp/bundled-loop-red.log`: prior Rust implementation emits repeated user messages, not the frozen guarded custom messages.
- Timer callback failures use the shared uncaught authority, not an alternate stderr-and-stop behavior.
- Gap: JS/libuv timer scheduling races and numeric timer extremes are outside the user-observable parity bar. Ordinary timer, deferral and shutdown behavior is mapped above.

- GREEN at `770ac1955`: eight focused tests pass (`/tmp/bundled-loop-green.log`), including eight frozen runtime scenarios and 14 command/tool cases. The older registration oracle retained real timers after each case; it now calls the registered shutdown handler and exposes `sendMessage`, removing its post-response TypeError without changing asserted results. Strict no-emit for the runtime oracle passes (`/tmp/bundled-loop-oracle-types.log`). Extension clippy in warning mode passes with no warnings on changed lines (`/tmp/bundled-clippy-warnings.log`); unchanged pre-existing conversion warnings remain.
