# Agent-core messages

This document covers the completed summary framing constants and value-only message records from `packages/agent/src/harness/messages.ts:19-29,40-45`. Earlier constant/record evidence below is separate from the bounded Q18 constructor/conversion work.

## Q18 native compaction context

- CompactionSummaryOptions.provider_native retains the original JsValue through message construction and serialization.
- convert_to_llm returns Result<Vec<Message>, AgentError>. Unsupported scalar opaque provider metadata is rejected explicitly at typed ProviderNativeContext conversion; a present value is never silently omitted on parse failure.
- Valid context value payloads retain native UTF16 strings and keys. The approved scalar-metadata gap does not authorize replacement or silent drops. See [message traceability](../porting/traceability/messages.md).

## What it must do

- [x] Expose `COMPACTION_SUMMARY_PREFIX`, `COMPACTION_SUMMARY_SUFFIX`, `BRANCH_SUMMARY_PREFIX`, and `BRANCH_SUMMARY_SUFFIX` as `pub const &str` values through `pi_agent_core::harness::messages`.
- [x] Re-export all four constants from `pi_agent_core::harness`, matching the TypeScript package-index export.
- [x] Preserve exact LF-only ASCII values and byte lengths `96`, `11`, `89`, and `10`.
- [x] Preserve the compaction suffix's leading LF and the branch suffix's lack of a leading LF.
- [x] Preserve arbitrary summary content when callers concatenate prefix, summary, and suffix; add no escaping, trimming, normalization, or dynamic formatting.

## Boundary and proof

- [x] Map TypeScript's inferred string-literal constants to Rust compile-time `&str` constants without a wrapper type.
- [x] Cover exact values, lengths, framing, module/root imports, and const usability in four focused tests in `crates/pi-agent-core/tests/summary_markers.rs`.
- [x] Compare request `13096` directly with the pinned TypeScript package-index runtime exports and strict exact literal-type proof.
- [x] Add the value-only `BashExecutionMessage` with closed `BashExecutionRole::BashExecution`, public mutable fields, `Debug` only, and module/root exports.
- [x] Cover role, field mutation, optional states, string boundaries, and binary64 bit preservation in five focused tests.
- [x] Compare direct differential requests `13097..=13104` and the strict public-index interface proof.
- [x] Add the value-only `BranchSummaryMessage` with closed `BranchSummaryRole::BranchSummary`, public mutable fields, `Debug` only, and module/root exports.
- [x] Cover role, field mutation, string boundaries, and timestamp binary64 bit preservation in four focused tests.
- [x] Compare direct differential requests `13105..=13112` and the strict public-index interface proof.

## How it works

- [Message marker traceability](../porting/traceability/messages.md)

## Implementation inventory

- `crates/pi-agent-core/src/harness/messages.rs` — four exact constants and Bash-execution/branch-summary value types.
- `crates/pi-agent-core/src/harness/mod.rs` — module and harness-root exports.
- `crates/pi-agent-core/tests/summary_markers.rs` — focused marker behavioral/API proof.
- `crates/pi-agent-core/tests/bash_execution_message.rs` — focused Bash-execution value/API proof.
- `crates/pi-agent-core/tests/branch_summary_message.rs` — focused branch-summary value/API proof.
- `crates/pi-agent-core/tests/differential_truncate/summary_markers.rs` — pinned marker request `13096`.
- `crates/pi-agent-core/tests/differential_truncate/bash_execution_message.rs` — eight Bash-execution plain-record groups (`13097..=13104`).
- `crates/pi-agent-core/tests/differential_truncate/branch_summary_message.rs` — eight branch-summary plain-record groups (`13105..=13112`).
- `oracle/truncate-oracle.ts` — persistent marker and value-record operations.
- `oracle/public-index-typecheck.mjs` — exact marker literal plus Bash-execution and BranchSummaryMessage interface proofs.
- `run-tests.sh` — focused test registration.

## Tests asserting this spec

- `crates/pi-agent-core/tests/summary_markers.rs`
- `crates/pi-agent-core/tests/bash_execution_message.rs`
- `crates/pi-agent-core/tests/branch_summary_message.rs`
- `crates/pi-agent-core/tests/differential_truncate.rs`
- `crates/pi-agent-core/tests/differential_truncate/summary_markers.rs`
- `crates/pi-agent-core/tests/differential_truncate/bash_execution_message.rs`
- `crates/pi-agent-core/tests/differential_truncate/branch_summary_message.rs`
- `oracle/public-index-typecheck.mjs`

## Proof status

- Summary-marker production implementation: commit `6f3a6cc`; focused proof has 4 passed tests.
- BashExecutionMessage production implementation: commit `143c635`; focused proof has 5 passed tests.
- BashExecutionMessage differential proof: commit `0fa04e1`; eight request groups (`13097..=13104`) pass against pinned TypeScript plain records.
- BranchSummaryMessage production implementation: commit `8e32578`; focused proof has 4 passed tests.
- BranchSummaryMessage differential proof: commit `fa8f6f2`; eight request groups (`13105..=13112`) pass against pinned TypeScript plain records, with strict public-index proof.
- BranchSummaryMessage readability proof: commit `0fad1f2` resolves both Rust fixture findings; `a991812` fixes hexadecimal grouping required by Clippy.
- BranchSummaryMessage final workspace proof at `a991812`: 226 passed, 0 failed, and 2 ignored; oracle/public-index, check, Clippy with denied warnings, formatting, documentation, source-pin, worktree, and diff gates pass.
- BashExecutionMessage readability proof: commit `07b6557` resolves the only TypeScript oracle finding; Rust, TypeScript, and JavaScript audits are clean.
- BashExecutionMessage final workspace proof at `07b6557`: 222 passed, 0 failed, and 2 ignored; oracle/public-index, check, Clippy with denied warnings, formatting, documentation, source-pin, worktree, and diff gates pass.

## Known gaps (current cycle)

None for the bounded value-only `BranchSummaryMessage` slice.

## BashExecutionMessage value contract

- Closed role enum: `BashExecutionRole::BashExecution`, with exact `as_str()` value `"bashExecution"`.
- Public mutable `BashExecutionMessage` derives `Debug` only.
- Fields, in source order: `role: BashExecutionRole`, `command: String`, `output: String`, `exit_code: Option<f64>`, `cancelled: bool`, `truncated: bool`, `full_output_path: Option<String>`, `timestamp: f64`, and `exclude_from_context: Option<bool>`.
- Required source `exitCode: undefined` maps to `None`; absent or explicitly undefined optional values map to `None`.
- `f64` fields preserve unrestricted binary64 bit patterns; no validation or normalization is added.
- Export through `pi_agent_core::harness::messages` and `pi_agent_core::harness` is required.

## BranchSummaryMessage value contract

- Closed role enum: `BranchSummaryRole::BranchSummary`, with exact `as_str()` value `"branchSummary"`.
- Public mutable `BranchSummaryMessage` derives `Debug` only.
- Fields, in source order: `role: BranchSummaryRole`, `summary: String`, `from_id: String`, and `timestamp: f64`.
- `timestamp` preserves unrestricted binary64 bit patterns; no validation or normalization is added.
- Export through `pi_agent_core::harness::messages` and `pi_agent_core::harness` is required.

## Out of scope

`bashExecutionToText`, `createBranchSummaryMessage`, Date or timestamp-string parsing, conversion, message unions, session/events, compaction/UI/RPC behavior, invalid runtime shapes, constructors, serde, validation, normalization, fallbacks, dependencies, other summary constructors, LLM conversion, serialization, compaction or session orchestration, escaping, trimming, newline normalization, and dynamic formatting.
