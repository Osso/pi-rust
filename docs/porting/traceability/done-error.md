# DISCREPANCY — custom Anthropic done/error

Source bug, preserved by parent classification2026-10-23. [Contract](../../specs/done-error.md).
Frozen `custom-provider-anthropic/index.ts:319–330,550` maps unknown stop reasons (including refusal) to error, then casts the reason to the nominal success union without runtime validation. Native previously fabricated an error event/message instead.

| Consumer/producer | Literal behavior | Proof |
|---|---|---|
| Custom Anthropic example | emit done/error with final message, no invented errorMessage | actual frozen terminal matrix RED in q15-examples-ui-acceptance.log; complete13-test GREEN in q15-anthropic-literal-green.log |
| Codex Responses terminal failed/cancelled status | emit done/error with final message, no invented errorMessage | [21-case terminal/status matrix](openai-codex-responses.md#discrepancies-preserved-literally) |
| Core proxy | accepts incoming done/error, assigns message.stopReason=error and forwards done | actual frozen RED→GREEN in q15-done-error-consumers-red-2.log / q15-done-error-consumers-green.log |
| Agent loop | ignores terminal reason; takes stream.result(), then handles final message.stopReason | actual frozen done_error scenario RED at enum decoding; GREEN in q15-done-error-consumers-green.log |
| Stream utility | both done/error terminate and resolve their carried message; never reject solely for done reason | proxy whole event/result differential above exercises the real utility; normally resolved Error-stop message retained |
| Print/RPC | print selects final-message stopReason and falls back to Request error; JSON/RPC forward session events | real frozen/native process proof GREEN: q15-json-print-done-error-green.log (text+JSON) and q15-json-event-and-rpc-gate.log (RPC full stream) |

Production DoneReason matches: proxy.stop_reason_for_done is the only exhaustive consumer. Event-stream extraction and agent-loop terminal branches ignore reason. All other DoneReason references construct terminals in built-in providers/faux; their existing StopReason gates remain unchanged. Custom-Anthropic raw event parsing, Codex terminal-status mapping and proxy reconstruction can produce the additional variant from frozen-producing paths. No generalized error→done mapping is added.

The native wire test `types::done_event_preserves_frozen_error_discrepancies_and_rejects_invalid_reasons` accepts and exactly round-trips done/error, while rejecting done/aborted and unknown reasons. Existing pending/deferred rejection assertions remain. Its former rejection of error was already RED on parent380346b4 with clean relevant paths (`q15e-parent-done-reason-red.log`): the earlier source-discrepancy merge introduced the stale-test failure, not the raw compat migration.

The example refresh callback also stops passing a synthetic AbortSignal to HTTP: frozen refreshAnthropicToken has no signal field. Actual eight-case OAuth differential caught true vs false fetch signal presence; GREEN with the complete13-test example gate. Old-only agent_settled and obsolete shared print/RPC shaping were removed before process parity was established. Broader RPC failures remain separate upstream prompt/usage work.
