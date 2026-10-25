# export-terminal.ts

Frozen `4f9e6f8a4`, `packages/coding-agent/src/core/export-terminal.ts` (179 lines). Rust: `crates/pi-coding-agent-ui/src/export_terminal.rs`; [contract](../../specs/export-terminal.md).

| Frozen symbol/branch | Native mapping | Proof |
|---|---|---|
| `ExportUI.requestRender` | no-op renderer callbacks, no terminal construction/start | CLI process returns output only |
| `sanitizeSavedText`, `sanitizeMessage`, `renderLine` | UTF-16 recursive value sanitization, strip ANSI/binary/C1 controls, normalize CRLF, retain generated SGR/reset each line | exact frozen oracle at widths 20/80 including OSC52, SGR, NUL, DEL and CRLF in saved text |
| `addUserMessage` | text/block concatenation, spacer and skill parsing | ordinary user GREEN; skill/image-only matrix pending |
| `addAssistantMessage`, `createHistoryToolComponent` | assistant widget, pending tool map, no images, aborted/error tool result | assistant text GREEN; tool matrix pending |
| `renderBashMessage` | bash widget with elapsed disabled and saved completion | completed bash GREEN; cancelled/truncated matrix pending |
| `addMessage` | user/assistant/toolResult/custom/summaries/bash dispatch | visible/hidden custom GREEN; summaries/tool-result matrix pending |
| `exportSessionToTerminal` | safe positive integer validation; load context entries; shared component render; LF termination | end-to-end CLI exact text GREEN; direct validation/context projection pending |
| frozen main `readTerminalExportSettings` / `exportTerminalHistory` | early CLI path, read-only in-memory settings snapshot, directory precedence, no control DB, theme init, completed stdout write | valid file/invalid incompatible flags/no filesystem writes GREEN; ID/settings/error paths pending |
| frozen args parse/validation | `export_terminal`, `export_columns`, intent detection, terminal-only diagnostic collector | 16-case frozen test written; pending post-implementation run |

RED: CLI fell through normal runtime startup, failed with `No models available` rather than rendering saved history (`/tmp/ext-api-export-render-red2.log`). GREEN: `cargo test -j3 -p pi-coding-agent --test suite_modes export_terminal::`: 2 tests, exact frozen output twice (`/tmp/ext-api-export-green5.log`), revision before initial export commit. No source edits after that run except docs/formatting. Existing helper proofs are separate.

## Expanded current proof

`/tmp/ext-api-export-matrix2.log`: 4 tests GREEN. Exact binary-vs-frozen ANSI matrix now covers dark widths20/80 and light width40, skill+followup, image-only user, mixed image/tool output (images disabled), successful/orphan/aborted/error tool calls, thinking, branch/compaction summaries, cancelled/truncated/excluded bash, visible/hidden custom entries. File-missing/header-invalid and direct width failures match frozen errors; unmatched ID exits4 and external alias exits1, without creating state. Saved settings and session bytes remain unchanged. This supersedes the earlier per-branch pending labels for those cases. Terminal Args 16-case matrix GREEN at 1d231dd3b (`/tmp/ext-api-args-green.log`). Successful ID lookup, directory precedence and compacted on-disk context projection remain pending; no fresh broad gate claim.

Gap: adversarial malformed messages/headers and numeric extremes beyond the safe-integer validation are outside the realistic-input parity bar. CLI argv stays scalar as the existing parser; saved text retains UTF-16 until stdout encoding. No TS source delta or dependency added.
