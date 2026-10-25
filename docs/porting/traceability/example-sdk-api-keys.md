# 09-api-keys-and-oauth.ts — common-pin audit

Frozen: `packages/coding-agent/examples/sdk/09-api-keys-and-oauth.ts`, `4f9e6f8a4`. Native: `crates/pi-coding-agent/examples/sdk/09_api_keys_and_oauth.rs`.

## Export and branch coverage

four ordered in-memory sessions: default storage, independent custom paths, runtime-only anthropic override sharing default auth, built-in-only registry sharing that auth; exact labels and disposal after each. Actual SDK state and whole-program stdout differential both remain active.

## Proof and limits

`example_sdk_api_keys_and_oauth` passes against the current frozen module: `/tmp/triage2-example_sdk_api_keys_and_oauth-audit.log`. [Detailed behavioral fixture mapping](sdk-examples.md). Audit revalidates the module logic and visible output, not live providers or OS-fault behavior. Native embedding/owned snapshot and en-US locale mappings remain within the standing approvals.

Gap: exotic/wrong-typed input, OS faults, numeric extremes and runtime scheduling oddities follow the current parity bar; existing precise discrepancies remain in the linked mapping. No new dependency, API carrier, or semantic deviation.
