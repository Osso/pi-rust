# rpc-extension-ui.ts — common-pin audit

Frozen: `packages/coding-agent/examples/rpc-extension-ui.ts`, `4f9e6f8a4`. Native: `crates/pi-coding-agent/examples/rpc_extension_ui.rs`.

## Export and branch coverage

OutputLog bounded append/raw/tail/UTF16 clip; loading dots lifecycle; prompt/select/input/editor dialogs; exact help/focus; startup spawn/write/exit; all extension_ui_request cases and response frames; local slash dialogs; agent/text/tool/error JSONL events, prompt trim, abort/idle Escape. Real private child and native rpc-demo transport fixtures retain process-lifetime proof.

## Proof and limits

`example_rpc_extension_ui` passes against the current frozen module: `/tmp/triage2-example_rpc_extension_ui-audit.log`. [Detailed behavioral fixture mapping](extension-examples-n.md). Audit revalidates the module logic and visible output, not live providers or OS-fault behavior. Native embedding/owned snapshot and en-US locale mappings remain within the standing approvals.

Gap: exotic/wrong-typed input, OS faults, numeric extremes and runtime scheduling oddities follow the current parity bar; existing precise discrepancies remain in the linked mapping. No new dependency, API carrier, or semantic deviation.
