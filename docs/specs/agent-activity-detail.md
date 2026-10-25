# Agent activity detail

Match frozen `core/agent-activity-detail.ts` at `4f9e6f8a4`.

- Inspect string arguments in this priority: command, code, path, file_path, pattern, query, url, prompt, message, agentId.
- Collapse ECMAScript whitespace runs to one space and trim. Skip empty and non-string values; return no detail when none qualifies.
- Return at most 160 UTF-16 units. For longer text, keep the first 159 units and append `…`, including a split surrogate when the cutoff bisects a pair.
- Preserve native UTF-16 text throughout. The agent-session caller owns activity publication.
