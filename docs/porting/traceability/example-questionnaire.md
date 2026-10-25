# questionnaire.ts — common-pin audit

Frozen: `packages/coding-agent/examples/extensions/questionnaire.ts`, `4f9e6f8a4`. Native: `crates/pi-coding-agent/examples/extensions/questionnaire.rs`.

## Export and branch coverage

registration/schema, no TUI/empty questions, label/allowOther defaults; single/multi tab and option movement; custom editor/trim/escape; answer insertion order, submit/cancel, render cache/invalidation, call/result renderers. Frozen :247 already guards missing selected options; the old postPin native-only bypass is removed. Raw custom-answer units and exact details/frames remain differential.

## Proof and limits

`suite_extensions example_questionnaire::` passes against the current frozen module: `/tmp/triage2-suite_extensions-audit.log`. [Detailed behavioral fixture mapping](extension-examples-n.md). Audit revalidates the module logic and visible output, not live providers or OS-fault behavior. Native embedding/owned snapshot and en-US locale mappings remain within the standing approvals.

Gap: exotic/wrong-typed input, OS faults, numeric extremes and runtime scheduling oddities follow the current parity bar; existing precise discrepancies remain in the linked mapping. No new dependency, API carrier, or semantic deviation.
