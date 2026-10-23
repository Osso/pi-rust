# Print-mode JSON events

Baseline: frozen `4f9e6f8a4`, `modes/json-event.ts`.

- `message_start.message` omits content, details, summary, output, providerNative and imageGenerationResult.
- `message_update` omits its message; assistantMessageEvent omits partial, content, toolCall, message and error.
- `turn_end` omits message/toolResults; `agent_end` omits messages.
- `tool_execution_end.result` omits content/details, retaining terminate and other properties.
- All other fields/events, native UTF-16 and remaining key order stay unchanged. Invalid JavaScript runtime input types remain excluded.

Only print JSON applies this shaping. Subscribed session events and RPC JSON remain complete. The old toJsonEvent helper is absent.
