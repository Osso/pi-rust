# Interactive session information

Authority: common-pin `interactive-mode.ts::handleSessionCommand` ([lock](../porting/source-lock.json)). The command is `/session`; do not introduce a `/stats` alias.

- Read current session statistics and session name. Render `Session Info`, optional truthy name, nullish file fallback `In-memory`, and session ID, then `Messages` with user/assistant/tool-call/tool-result/total counts.
- Render `Tokens` with input and output separately. Add cache-read/cache-write lines only when positive, then total. Use the single shared default en-US number formatter, preserving fractional counts and signed/non-finite semantics.
- Add `Cost` only for a positive cost, formatting total to four decimal places with no dollar sign. Do not show per-model cost or cache-waste/re-billing sections.
- Append one Spacer(1) and one Text(info,1,0) to chat, then request rendering. Apply source theme styles to headings/labels only; do not replace this with the outer dimmed status-message path. Preserve raw UTF-16 names/paths through wrapping.
- The common pin has no cache-stats module, cache-miss notices or configurable cache-notice flag; those native paths remain removed. Legacy cache-notice keys remain ordinary unknown settings data. The separately authorized historical Mermaid setting is implemented; see [settings](settings-manager.md) and [renderer trace](../porting/traceability/interactive-mermaid.md).

[Source-handler proof](../porting/traceability/interactive-mode.md#session-info-and-old-only-cache-removal-2026-10-22) · [Removed module classification](../porting/traceability/cache-stats.md) · [Shared number proof](../porting/traceability/js-compat.md).
