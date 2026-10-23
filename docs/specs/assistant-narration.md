# Assistant public narration and rendering

Authorities: common-pin `core/assistant-narration.ts` and `modes/interactive/components/assistant-message.ts` ([lock](../porting/source-lock.json)).

- Public prose is every text block, plus thinking whose Base64/protobuf envelope contains field 2 → field 1 → field 8 decoding to exactly `narration`. Preserve original signed thinking blocks for replay; this marker classifier does not verify cryptographic signatures.
- Reuse shared Node Buffer Base64 semantics. Preserve first matching length-delimited field, wire 0/1/2/5 skipping, malformed-input false results, five-byte varints and JS uint32 field-key coercion. Opaque signatures retain the approved Unicode-scalar boundary; human text remains lossless UTF-16.
- Render each content position independently. Public narration uses normal Markdown even when thinking is hidden. Private nonempty thinking gets its own Markdown or hidden label; do not coalesce adjacent thinking blocks. Empty slots remain invisible. Add a thinking spacer only when later visible content exists.
- Input messages follow the approved owned-value/event-snapshot boundary. The controller must supply current snapshots when a built-in action relies on source message-content aliasing; that adaptation belongs to its own mode slice.
- Retain unchanged Markdown/Text components and their caches across updates and setters. Rebuild child ordering without replacing unchanged components. Invalidation invalidates current children before reconciling the cached message. Preserve inherited Container behavior.
- Retain the leading spacer when visible content exists, and reconcile trailing status. Length has the exact frozen maximum-output-token notice, even with tool calls; tool calls suppress abort/error status. Empty error messages use frozen fallback wording.
- Nonempty output without tool calls gets frozen OSC133 A/B/C markers. Setters immediately reconcile the cached message. Constructor/update APIs have no old transformer or streaming parameters.
- Print mode classifies and prints the same public prose without trimming it, including empty public blocks once a message is selected. Apply Node UTF-8 only at stdout. Interactive thinking-end refreshes when the final signature identifies narration, even with hide-thinking enabled; missing/non-thinking/out-of-range content stays invisible.

[Core proof](../porting/traceability/assistant-narration.md) · [Component proof](../porting/traceability/interactive-assistant-message.md).
