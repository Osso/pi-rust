# Interactive name and inventory commands

Match frozen `interactive-mode.ts` name/unname/tools/extensions handlers. [Proof](../porting/traceability/interactive-mode.md).

## What it must do

- [x] `/name` without an argument shows the current nonempty name, otherwise the frozen usage warning. Setting a name shows the stored name and the literal JSON-quoted normalization warning when it differs from the requested trimmed text.
- [x] `/unname` clears metadata and appends `Session name removed`, including repeated calls. Name setters retain normal session-info events; handlers do not update terminal titles independently of those events. Setter failures propagate unchanged to the caller.
- [x] `/tools` and `/extensions` append the shared CLI inventory text, using one formatter authority. Repeated commands append output instead of replacing a coalesced status. Active tools and loaded extension metadata are read at each call.

## Tests asserting this spec

`interactive_display_commands.rs` and `interactive-display-commands-oracle.mts` compare actual frozen handlers/formatters and SessionManager operations with native modes: complete UTF-16 frames at widths25/120, normalization/current-name queries, reopened persisted names, session-info events, terminal-title effects, active-tool changes and populated/empty extension inventories.

## Boundaries

Source session emission is observed through a recording sink; native emission uses a real subscription. This proves handler behavior, not whole-mode event routing. Existing session-info formatting remains unchanged. No external state or dependencies are added.
