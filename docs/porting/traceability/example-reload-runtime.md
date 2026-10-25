# reload-runtime.ts — common-pin audit

Frozen: `packages/coding-agent/examples/extensions/reload-runtime.ts`, `4f9e6f8a4`. Native: `crates/pi-coding-agent/examples/extensions/reload_runtime.rs`.

## Export and branch coverage

reload-runtime command awaits ctx.reload and returns; reload_runtime tool registers exact schema/metadata, queues /reload-runtime as followUp, returns exact content/details. No old prompt snippet remains.

## Proof and limits

`example_reload_runtime` passes against the current frozen module: `/tmp/triage2-example_reload_runtime.log`. [Detailed behavioral fixture mapping](extension-examples-d.md). Audit revalidates the module logic and visible output, not live providers or OS-fault behavior. Native embedding/owned snapshot and en-US locale mappings remain within the standing approvals.

Gap: exotic/wrong-typed input, OS faults, numeric extremes and runtime scheduling oddities follow the current parity bar; existing precise discrepancies remain in the linked mapping. No new dependency, API carrier, or semantic deviation.
