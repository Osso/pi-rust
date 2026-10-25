# qna.ts — common-pin audit

Frozen: `packages/coding-agent/examples/extensions/qna.ts`, `4f9e6f8a4`. Native: `crates/pi-coding-agent/examples/extensions/qna.rs`.

## Export and branch coverage

TUI/model gates; reverse branch scan and incomplete stop reason; skip nontext assistant entries, join text; loader/auth/complete/aborted/failure; cancel notification or exact editor text and success notification. The oracle now registers faux through actual frozen ModelRegistry.registerProvider.

## Proof and limits

`example_qna` passes against the current frozen module: `/tmp/triage2-final-example_qna.log`. [Detailed behavioral fixture mapping](extension-examples-f.md). Audit revalidates the module logic and visible output, not live providers or OS-fault behavior. Native embedding/owned snapshot and en-US locale mappings remain within the standing approvals.

Gap: exotic/wrong-typed input, OS faults, numeric extremes and runtime scheduling oddities follow the current parity bar; existing precise discrepancies remain in the linked mapping. No new dependency, API carrier, or semantic deviation.
