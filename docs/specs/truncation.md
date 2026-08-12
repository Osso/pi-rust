# Agent-core output truncation

Agent-core truncation ports `packages/agent/src/harness/utils/truncate.ts` from the pinned TypeScript source. It provides dependency-free size formatting and deterministic head, tail, and single-line truncation for later read, bash, grep, and find tools.

## What it must do

### Limits and formatting

- [x] Expose default limits of 2,000 lines, 10,240 bytes, and 500 Unicode scalar values per grep line.
- [x] Resolve omitted Rust options through `TruncationOptions::default` while preserving explicit zero limits.
- [x] Format non-negative integer byte counts with source B, KB, and MB thresholds and one-decimal half-up rounding.

### Shared metadata

- [x] Count UTF-8 bytes and split lines only on LF.
- [x] Exclude one terminal empty segment from line counts when content ends in LF.
- [x] Return original content, including a trailing LF, when both limits fit.
- [x] Return complete source and output counts, applied limits, truncation cause, and partial-line flags.

### Head truncation

- [x] Keep only complete leading lines within both limits.
- [x] Return empty output with `first_line_exceeds_limit` when the first line alone exceeds the byte limit.
- [x] Prefer source line-limit classification when the retained line count reaches `max_lines` while bytes still fit.
- [x] Drop a source trailing LF from truncated output.

### Tail truncation

- [x] Keep complete trailing lines within both limits.
- [x] If the final original line alone exceeds the byte limit, keep its largest Unicode-scalar-safe suffix.
- [x] Preserve the source metadata distinction where an empty partial suffix still counts as one output line.
- [x] Prefer source line-limit classification when the partial output also reaches `max_lines`.
- [x] Drop a source trailing LF from truncated output.

### Single-line truncation

- [x] Keep lines at or below the selected Unicode scalar limit unchanged.
- [x] Append the exact `... [truncated]` marker after the retained prefix.
- [x] Never split a Rust Unicode scalar.

## How it works

- [Truncation traceability](../porting/traceability/truncation.md)

## Implementation inventory

- `crates/pi-agent-core/src/harness/utils/truncate.rs` — constants, public result types, formatting, and truncation algorithms.
- `crates/pi-agent-core/src/harness/utils/mod.rs` — public utility exports.
- `crates/pi-agent-core/tests/truncate.rs` — focused shared-domain behavior and approved Unicode policy.
- `oracle/truncate-oracle.ts` — persistent adapter over normal and Buffer-disabled pinned TypeScript execution.
- `crates/pi-agent-core/tests/differential_truncate/` — targeted full-metadata, fallback, discrepancy, and deterministic-corpus proof.

## Tests asserting this spec

- `crates/pi-agent-core/tests/truncate.rs`
- `crates/pi-agent-core/tests/differential_truncate.rs`
- `crates/pi-agent-core/tests/differential_truncate/format_size.rs`
- `crates/pi-agent-core/tests/differential_truncate/targeted.rs`
- `crates/pi-agent-core/tests/differential_truncate/truncate_line.rs`
- `crates/pi-agent-core/tests/differential_truncate/tail_corpus.rs`

## Known gaps (current cycle)

None for this bounded truncation slice.

## Out of scope

- Negative, fractional, NaN, and infinite JavaScript limits and their source bugs. Alessio approved non-negative integer Rust limits on August 12, 2026.
- JavaScript lone or malformed UTF-16 surrogate inputs, which Rust `str` cannot represent.
- JavaScript UTF-16 code-unit slicing in `truncateLine`. Alessio approved Unicode-scalar counting on August 12, 2026, so Rust never returns a lone surrogate.
- Byte counts and limits above the exact JavaScript safe-integer domain.
- File reads, shell execution, path resolution, truncation notices, and tool-specific output formatting.
- The distinct Node-specific truncation implementation under `packages/coding-agent`; it requires separate source mapping and proof.
