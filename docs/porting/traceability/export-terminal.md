# export-terminal.ts

Frozen `4f9e6f8a4`, `packages/coding-agent/src/core/export-terminal.ts` (179 lines).
Native: `crates/pi-coding-agent-ui/src/export_terminal.rs`; [contract](../../specs/export-terminal.md).
Whole-module audit closed 2026-10-25. Current commands and revisions: [extension API proof](../ext-api-repin-proof.md).

| Frozen symbol/branch | Native mapping | Current proof |
| --- | --- | --- |
| ExportUI.requestRender | no-op render callbacks; no started terminal | CLI subprocess emits only exported text |
| sanitizeSavedText / sanitizeMessage / renderLine | recursive native JSON text sanitization, strip saved ANSI/binary/C1 controls, CRLF normalization; retain generated SGR and reset each line | exact frozen ANSI output at dark widths20/80 and light width40, including OSC52, SGR, NUL, DEL, CRLF |
| addUserMessage | concatenate text blocks; skip empty/image-only; spacer; skill and followup widgets | mixed history differential |
| addAssistantMessage / createHistoryToolComponent | assistant widget plus pending tool components, no images | successful/orphan/aborted/error tool calls, thinking and mixed image/text results |
| renderBashMessage | completed bash widget with elapsed disabled | completed/cancelled/truncated/excluded bash and output-path messages |
| addMessage | user/assistant/toolResult/custom/branchSummary/compactionSummary/bash dispatch | visible/hidden custom entries, explicit summaries, pending-map updates |
| exportSessionToTerminal | positive safe integer validation, compaction-aware context projection, shared widgets, LF termination | direct width failures; missing/invalid files; on-disk compaction removes pre-kept history and retains summary/kept/after entries |
| main readTerminalExportSettings / exportTerminalHistory consumer | early read-only CLI branch, in-memory settings, theme initialization, completed stdout write | successful ID-prefix lookup; explicit session-dir over environment over settings; bytes unchanged; no control DB/settings/session writes |
| CLI terminal-only args | parser/diagnostic collection | earlier 16-case actual-frozen args matrix; incompatible flags reject before runtime startup |
| missing ID / external aliases | read-only resolver failures | exact exit4/exit1 diagnostics and no created state |

`cargo test -j3 -p pi-coding-agent --test suite_modes export_terminal:: -- --nocapture`: five parents GREEN, `/tmp/ext-api3-final-terminal.log`. This renews every saved-history, directory-precedence and compacted-context case, not just ordinary text. The initial RED was normal runtime startup (`No models available`) instead of export output; `/tmp/ext-api-export-render-red2.log`.

No source delta or dependency added. Gap: adversarial malformed message/header shapes, exceptional stdio/OS failures and numeric extremes outside the existing safe-integer check are outside the realistic-input parity bar. CLI argv retains its existing scalar parser; saved human text remains UTF-16 until stdout encoding.
