# CompactionDetails

This specification covers the module-local `CompactionDetails` interface from `packages/agent/src/harness/compaction/compaction.ts:30-35`.

## Required behavior

- Expose `pi_agent_core::harness::compaction::CompactionDetails` with public mutable fields in source order: `read_files: Vec<String>` and `modified_files: Vec<String>`.
- Derive `Debug` only. Preserve empty vectors, order, duplicates, empty strings, Unicode, NUL, punctuation, and embedded/trailing newlines.
- Do not add a harness-root or package re-export because the pinned package index omits `CompactionDetails`.
- Differential proof compares final source-shaped plain records through exactly eight request groups `13156..=13163`; the oracle does not implement extraction or compaction behavior.

## Verification closure

- [x] Current-HEAD inherited verification: focused 2 passed/0 failed, differential 1 passed/0 failed, oracle/fmt/diff exit 0; saved workspace 254 passed/0 failed/2 ignored plus check and all-feature Clippy with `-D warnings` pass. `OVERALL: PASS`.

## Out of scope

File-operation extraction, session entries, compaction orchestration, `CompactResult`, constructors, serialization, validation, package/root exports, and unrelated compaction APIs remain deferred.

## Proof inventory

- Focused: `crates/pi-agent-core/tests/compaction_details.rs`
- Differential: `crates/pi-agent-core/tests/differential_truncate/compaction_details.rs`
- Direct pinned type proof: `oracle/compaction-details-typecheck.mjs`
- Oracle adapter: `oracle/truncate-oracle.ts`
