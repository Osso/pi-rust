# Agent-core message markers

This bounded slice ports the four public summary framing constants from `packages/agent/src/harness/messages.ts:4-17`. Message structs, constructors, conversion, and serialization remain deferred. See [message marker traceability](../porting/traceability/messages.md).

## What it must do

- [x] Expose `COMPACTION_SUMMARY_PREFIX`, `COMPACTION_SUMMARY_SUFFIX`, `BRANCH_SUMMARY_PREFIX`, and `BRANCH_SUMMARY_SUFFIX` as `pub const &str` values through `pi_agent_core::harness::messages`.
- [x] Re-export all four constants from `pi_agent_core::harness`, matching the TypeScript package-index export.
- [x] Preserve exact LF-only ASCII values and byte lengths `96`, `11`, `89`, and `10`.
- [x] Preserve the compaction suffix's leading LF and the branch suffix's lack of a leading LF.
- [x] Preserve arbitrary summary content when callers concatenate prefix, summary, and suffix; add no escaping, trimming, normalization, or dynamic formatting.

## Boundary and proof

- [x] Map TypeScript's inferred string-literal constants to Rust compile-time `&str` constants without a wrapper type.
- [x] Cover exact values, lengths, framing, module/root imports, and const usability in `crates/pi-agent-core/tests/summary_markers.rs`.
- [x] Compare request `13096` directly with the pinned TypeScript package exports.

## How it works

- [Message marker traceability](../porting/traceability/messages.md)

## Implementation inventory

- `crates/pi-agent-core/src/harness/messages.rs` — four exact constants.
- `crates/pi-agent-core/src/harness/mod.rs` — module and harness-root exports.
- `crates/pi-agent-core/tests/summary_markers.rs` — focused behavioral/API proof.
- `crates/pi-agent-core/tests/differential_truncate/summary_markers.rs` — pinned request `13096`.
- `oracle/truncate-oracle.ts` — persistent package-index marker operation.
- `oracle/public-index-typecheck.mjs` — exact literal-type and package-export proof.
- `run-tests.sh` — focused test registration.

## Tests asserting this spec

- `crates/pi-agent-core/tests/summary_markers.rs`
- `crates/pi-agent-core/tests/differential_truncate.rs`
- `crates/pi-agent-core/tests/differential_truncate/summary_markers.rs`

## Known gaps (current cycle)

None for this bounded marker slice.

## Out of scope

Message structs, summary constructors, LLM conversion, serialization, compaction or session orchestration, escaping, trimming, newline normalization, dynamic formatting, fallbacks, and new dependencies.
