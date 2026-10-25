# index.ts — common-pin audit

Frozen: `packages/coding-agent/examples/extensions/subagent/index.ts`, `4f9e6f8a4`. Native: `crates/pi-coding-agent/examples/extensions/subagent/index.rs and subagent/render.rs`.

## Export and branch coverage

token/usage/tool/result formatting and text extraction; exact UTF16 output caps; ordered concurrency (4)/task cap (8); private prompt file and child argv; stdout JSONL/stderr, usage/model/stop/error tracking, abort and cleanup; exclusive single/parallel/chain modes, scope/trust confirmation, replacement-string chaining, result details and expanded/collapsed renderers. Child --model is explicit agent configuration only; inherited model/thinking injection was removed in the predecessor.

## Proof and limits

`example_subagent` passes against the current frozen module: `/tmp/triage2-example_subagent.log`. [Detailed behavioral fixture mapping](extension-examples-n.md). Audit revalidates the module logic and visible output, not live providers or OS-fault behavior. Native embedding/owned snapshot and en-US locale mappings remain within the standing approvals.

Gap: exotic/wrong-typed input, OS faults, numeric extremes and runtime scheduling oddities follow the current parity bar; existing precise discrepancies remain in the linked mapping. No new dependency, API carrier, or semantic deviation.
