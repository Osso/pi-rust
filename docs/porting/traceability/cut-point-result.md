# Cut-point result traceability

Pinned source: `packages/agent/src/harness/compaction/compaction.ts` at commit `532f7d12cd19235081bd4d6e425bde185652d86f`.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `compaction.ts:450-457` | Public mutable required interface with `firstKeptEntryIndex: number`, `turnStartIndex: number`, and `isSplitTurn: boolean` | Module-local `harness::compaction::CutPointResult` with public `f64`, `f64`, and `bool` fields in source order; focused construction, mutation, Debug, and binary64 proof; eight direct source-shaped differential groups `13130..=13137` |
| `index.ts:58-75` | Package index exports `findCutPoint` but omits `type CutPointResult` | No `harness`-root or package stand-in; module-only Rust path preserves the selected source boundary |

## Rust boundary

- `CutPointResult` derives `Debug` only and remains publicly mutable.
- TypeScript `number` fields map to unrestricted Rust `f64` values; no integer or `usize` narrowing is added.
- The semantic `-1` turn-start sentinel and all representable binary64 bit patterns remain valid values.
- Invalid JavaScript runtime shapes and lone-surrogate strings remain outside the typed Rust boundary.

## Proof status

- Focused RED: `/tmp/pi-rust-cut-point-result-red.out` captures exit 101 with only the missing module-type import; the proof ledger records the command and result.
- Focused GREEN: `/tmp/pi-rust-cut-point-result-green.out` captures 3 passed and 0 failed; the proof ledger records exit 0.
- Formatting and diff checks exit 0; Rust readability reports no threshold violations for the changed module and focused test.
- Differential GREEN: `/tmp/pi-rust-cut-point-result-differential-green.out` records exit 0 with one persistent test passing exactly eight request groups (`13130..=13137`).
- Direct-module type proof: `/tmp/pi-rust-cut-point-result-oracle.out` records `./run-tests.sh oracle` exit 0; `oracle/cut-point-result-typecheck.mjs` validates exact shape, required fields, public mutation, and unrestricted number assignments without public-index proof.
- Node syntax: `/tmp/pi-rust-cut-point-result-node-check.out` records exit 0.
- Readability: `/tmp/pi-rust-cut-point-result-readability-full` reports no new threshold violations in the changed Rust fixture/runner or standalone typeproof; the existing `executeUtilityRequest` finding remains baseline-identical, while new special dispatch is below thresholds.
- Final workspace/static/docs/clean-pin gates and inherited verification remain pending.

## Exclusions

`findCutPoint`, `findTurnStartIndex`, entry/session/message unions, cut-point discovery, protected-suffix/token logic, compaction preparation/orchestration, constructors, serialization, validation, normalization, integer narrowing, package or harness-root exports, the coding-agent duplicate, dependencies, and unrelated compaction APIs remain deferred. Differential requests normalize final source-shaped records; direct typeproof assignments prove mutability separately and do not claim runtime construction or validation.
