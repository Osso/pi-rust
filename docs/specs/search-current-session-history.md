# Current session history search

Frozen contract: `4f9e6f8a4`, `core/tools/search-current-session-history.ts`.

Search persisted active-branch conversational entries, including compacted ancestors, by trimmed case-insensitive literal query. Match user/assistant text, non-self tool names and recursively enumerated argument keys/values, non-self tool-result text, bash command/output, custom-message text and summary text. Exclude thinking/signatures/images and this tool’s own calls/results. Excluded content remains visible when its entry is selected as neighboring context.

Return chronological paginated matching entries with neighboring searchable entries, exact source details/pretty JSON, compacted flags and cursor. Preserve schema, parallel execution metadata, guidance and renderers.

[Trace and proof](../porting/traceability/search-current-session-history.md).
