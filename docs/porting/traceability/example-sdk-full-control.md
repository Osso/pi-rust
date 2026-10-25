# 12-full-control.ts — common-pin audit

Frozen: `packages/coding-agent/examples/sdk/12-full-control.ts`, `4f9e6f8a4`. Native: `crates/pi-coding-agent/examples/sdk/12_full_control.rs`.

## Export and branch coverage

custom auth path; nonempty MY_ANTHROPIC_KEY runtime override; in-memory model registry; Sonnet lookup/error; compaction/retry overrides; no-discovery ResourceLoader with exact prompt and read/bash; memory session, off thinking, text-delta subscription/prompt/newline/finally. Whole-program and actual SDK-state fixtures prove custom prompt and models.json exclusion.

## Proof and limits

`example_sdk_full_control` passes against the current frozen module: `/tmp/triage2-example_sdk_full_control-audit.log`. [Detailed behavioral fixture mapping](sdk-examples.md). Audit revalidates the module logic and visible output, not live providers or OS-fault behavior. Native embedding/owned snapshot and en-US locale mappings remain within the standing approvals.

Gap: exotic/wrong-typed input, OS faults, numeric extremes and runtime scheduling oddities follow the current parity bar; existing precise discrepancies remain in the linked mapping. No new dependency, API carrier, or semantic deviation.
