# Session statistics

Frozen `agent-session.ts:6233–6280` at4f9e6f8a4 reports the current Agent message state, not all persisted history.

- Count user/assistant/tool-result roles and assistant tool calls in current messages; total includes every current message.
- Sum input/output/cacheRead/cacheWrite and cost from assistant usage only.
- After tree navigation rebuilds Agent messages, statistics reflect the selected branch immediately; persisted history remains intact.
- Preserve current session file/ID and context-usage reporting.

[Traceability](../porting/traceability/agent-session.md). `differential_agent_session` compares real frozen multi-turn events, pre/post-navigation statistics, JSONL and selected messages.
