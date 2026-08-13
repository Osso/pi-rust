# Summarization system prompt traceability

Pinned source: `packages/agent/src/harness/compaction/compaction.ts` at commit `532f7d12cd19235081bd4d6e425bde185652d86f`.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `compaction.ts:489-491` | Module-local static template literal with exact 310-byte ASCII value, three lines, two LF characters, one blank middle line, no CR, trailing whitespace, or trailing LF | `harness::compaction::SUMMARIZATION_SYSTEM_PROMPT`; focused exact-value, byte/character/UTF-16 length, line-position, ASCII, whitespace, CR, and const-usage tests; direct differential request `13129` and direct-module typeproof |
| `index.ts:58-75` | Package index exports `shouldCompact` and adjacent compaction APIs but omits `SUMMARIZATION_SYSTEM_PROMPT` | No `harness`-root or package stand-in; module-only export is preserved and direct-module typeproof does not import the package index |
| `coding-agent/src/core/compaction/utils.ts:196-198` | Separate coding-agent duplicate is outside the selected agent-core module boundary | No Rust duplicate is added |

## Rust boundary

- The constant is a direct `pub const &str`; no wrapper, function, dynamic construction, interpolation, formatting, normalization, or dependency is added.
- The value is ASCII and LF-only. Its blank line and absence of trailing LF are part of the contract.
- The selected module path is `pi_agent_core::harness::compaction::SUMMARIZATION_SYSTEM_PROMPT`; `pi_agent_core::harness` does not re-export it.

## Proof status

- Focused RED: `/tmp/pi-rust-summarization-system-prompt-red.out` records exit 101 with only the missing module-constant import.
- Focused GREEN: `/tmp/pi-rust-summarization-system-prompt-green.out` records exit 0 with 4 passed and 0 failed; production commit `017da73` adds the exact module constant, focused proof, and documentation paths.
- Differential RED: `/tmp/pi-rust-summarization-system-prompt-differential-red.out` records the valid unsupported-operation boundary at request `13129` after requests through `13128` completed.
- Differential GREEN: `/tmp/pi-rust-summarization-system-prompt-differential-green.out` records exit 0 with the persistent test passing request `13129`.
- Direct proof commit: `test(agent): compare summarization system prompt` adds the differential fixture, runtime adapter, and direct-module typeproof.
- Oracle/typeproof: `/tmp/pi-rust-summarization-system-prompt-oracle.out` records exit 0 for existing oracle typechecks plus direct-module exact-literal proof; `node --check oracle/summarization-system-prompt-typecheck.mjs` also exits 0.
- Formatting and diff checks exit 0. Rust, TypeScript, and JavaScript readability report no new findings; changed oracle dispatch was extracted while pre-existing `executeUtilityRequest` remains unchanged.
- Final workspace/static gates and inherited verification remain pending.

## Exclusions

`SUMMARIZATION_PROMPT`, prompt construction, compaction and branch orchestration, model calls, retries, serialization, token estimation, session behavior, the coding-agent duplicate, package/root exports, dynamic formatting, dependencies, and unrelated compaction APIs remain deferred.
