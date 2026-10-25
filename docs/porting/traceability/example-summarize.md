# summarize.ts — common-pin audit

Frozen `4f9e6f8a4`, 207 lines; native `examples/extensions/summarize.rs`.

| Symbols / branches | Native / proof |
| --- | --- |
| extractTextParts/extractToolCallLines/buildConversationText | text_parts/conversation_text: string/block input, user/assistant filtering, whitespace trim, text join, tool argument JSON and numeric key order, skipped non-message entries |
| buildSummaryPrompt | complete_summary: literal heading/conversation markers and request options |
| showSummaryUi | Summary: TUI-only border/title/Markdown/help, Enter/Escape close and invalidation |
| registration and command gates | summarize_extension/run_summary: empty history, hasUI notifications, missing model/auth, complete failure, joined response text |

`example_summarize` compares ten command scenarios through the actual source loader/runner, private faux transport and real UI components. Additional `example_conversation_text` actual-source pure-helper differential covers unmatched units in user/assistant text, compaction summary and nested tool argument values. RED `/tmp/triage2-conversation-text-red3.log` showed scalar serde rejecting existing raw text; the repaired helper and complete request now use JsString/JsValue throughout. Source factory/import seams are not invoked by the pure-helper probe. [Historical command matrix](extension-examples-i.md).

Gap: wrongly typed/exotic input and OS/runtime faults follow the standing parity bar. No dependency or behavior deviation.
