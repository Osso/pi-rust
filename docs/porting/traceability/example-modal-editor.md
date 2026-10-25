# modal-editor.ts — common-pin audit

Frozen: `packages/coding-agent/examples/extensions/modal-editor.ts`, `4f9e6f8a4`. Native: `crates/pi-coding-agent/examples/extensions/modal_editor.rs`.

## Export and branch coverage

session_start installs composed CustomEditor; insert/normal Escape transitions; h/j/k/l/0/$/x mapping; i/a return to insert with append-right; printable rejection and control pass-through; empty/narrow/ANSI mode label. Object.hasOwn is already in the common pin, so inherited-property key strings pass through without a post-pin exception.

## Proof and limits

`suite_tool_data extension_examples_editors::modal_editor` passes against the current frozen module: `/tmp/triage2-suite_tool_data-final-dev.log`. [Detailed behavioral fixture mapping](extension-examples-editors.md). Audit revalidates the module logic and visible output, not live providers or OS-fault behavior. Native embedding/owned snapshot and en-US locale mappings remain within the standing approvals.

Gap: exotic/wrong-typed input, OS faults, numeric extremes and runtime scheduling oddities follow the current parity bar; existing precise discrepancies remain in the linked mapping. No new dependency, API carrier, or semantic deviation.
