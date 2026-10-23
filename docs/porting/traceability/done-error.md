# DISCREPANCY — custom Anthropic done/error

Source bug, preserved by parent classification2026-10-23. [Contract](../../specs/done-error.md).
Frozen `custom-provider-anthropic/index.ts:319–330,550` maps unknown stop reasons (including refusal) to error, then casts the reason to the nominal success union without runtime validation. Native previously fabricated an error event/message instead.

| Consumer/producer | Literal behavior | Proof |
|---|---|---|
| Custom Anthropic example | emit done/error with final message, no invented errorMessage | actual frozen terminal matrix RED in q15-examples-ui-acceptance.log; GREEN pending |
| Core proxy | accepts incoming done/error, assigns message.stopReason=error and forwards done | actual frozen RED in q15-done-error-consumers-red-2.log; GREEN pending |
| Agent loop | ignores terminal reason; takes stream.result(), then handles final message.stopReason | actual frozen done_error scenario RED at enum decoding; GREEN pending |
| Stream utility | both done/error terminate and resolve their carried message; never reject solely for done reason | source/native audit; dedicated proof pending |
| Print/RPC | print selects final-message stopReason and falls back to Request error; JSON/RPC forward session events | frozen print-mode.ts:143–149, rpc-mode.ts:372–374; process proof pending |

Production DoneReason matches: proxy.stop_reason_for_done is the only exhaustive consumer. Event-stream extraction and agent-loop terminal branches ignore reason. All other DoneReason references construct terminals in built-in providers/faux; their existing StopReason gates remain unchanged. Only custom-Anthropic raw event parsing and proxy reconstruction can produce the additional variant from frozen-producing paths. No generalized error→done mapping is added.

The example refresh callback also stops passing a synthetic AbortSignal to HTTP: frozen refreshAnthropicToken has no signal field. Actual eight-case OAuth differential caught true vs false fetch signal presence; GREEN pending.
