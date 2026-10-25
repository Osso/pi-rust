# message-renderer.ts — common-pin audit

Frozen: `packages/coding-agent/examples/extensions/message-renderer.ts`, `4f9e6f8a4`. Native: `crates/pi-coding-agent/examples/extensions/message_renderer.rs`.

## Export and branch coverage

status command: JS trim/split, warn/error prefix, default content and clock; status-update renderer: default level, success/warning/error role, uppercase prefix, truthy expanded timestamp, Box/Text padding and background.

Audit repair: a valid block-array custom message previously panicked. Frozen interpolates the array with JavaScript string conversion (`[object Object],[object Object]`), rather than extracting block text. The native renderer now uses the existing shared conversion. RED `/tmp/triage2-message-blocks-red.log` records the panic; the expanded source/native frame differential keeps this source behavior literal.

## Proof and limits

`example_message_renderer` passes against the current frozen module: `/tmp/triage2-message-renderer-proof.log`. [Detailed behavioral fixture mapping](extension-examples-d.md). Audit revalidates the module logic and visible output, not live providers or OS-fault behavior. Native embedding/owned snapshot and en-US locale mappings remain within the standing approvals.

Gap: exotic/wrong-typed input, OS faults, numeric extremes and runtime scheduling oddities follow the current parity bar; existing precise discrepancies remain in the linked mapping. No new dependency, API carrier, or semantic deviation.
