# Summarization system prompt traceability

Pinned source: `packages/agent/src/harness/compaction/compaction.ts` at commit `532f7d12cd19235081bd4d6e425bde185652d86f`.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `compaction.ts:489-491` | Module-local static template literal with exact 310-byte ASCII value, three lines, two LF characters, one blank middle line, no CR, trailing whitespace, or trailing LF | `harness::compaction::SUMMARIZATION_SYSTEM_PROMPT`; focused exact-value, byte/character/UTF-16 length, line-position, ASCII, whitespace, CR, and const-usage tests; direct differential request `13129` and direct-module typeproof |
| `index.ts:58-75` | Package index exports `shouldCompact` and adjacent compaction APIs but omits `SUMMARIZATION_SYSTEM_PROMPT` | No `harness`-root or package stand-in; module-only export is preserved and the standalone direct-module type proof makes no public-index claim |
| `coding-agent/src/core/compaction/utils.ts:196-198` | Separate coding-agent duplicate is outside the selected agent-core module boundary | No Rust duplicate is added |

## Rust boundary

- The constant is a direct `pub const &str`; no wrapper, function, dynamic construction, interpolation, formatting, normalization, or dependency is added.
- The value is ASCII and LF-only. Its blank line and absence of trailing LF are part of the contract.
- The selected module path is `pi_agent_core::harness::compaction::SUMMARIZATION_SYSTEM_PROMPT`; `pi_agent_core::harness` does not re-export it.

## Proof status

- Focused RED: `/tmp/pi-rust-summarization-system-prompt-red.out` captures only the missing module-constant import; the proof ledger records exit 101.
- Focused GREEN: `/tmp/pi-rust-summarization-system-prompt-green.out` captures 4 passed and 0 failed; the proof ledger records exit 0. Production commit `017da73` adds the exact module constant, focused proof, and documentation paths.
- Differential RED: `/tmp/pi-rust-summarization-system-prompt-differential-red.out` captures the valid unsupported-operation boundary at request `13129` after requests through `13128` completed; the proof ledger records exit 101.
- Differential GREEN: `/tmp/pi-rust-summarization-system-prompt-differential-green.out` captures the persistent test passing request `13129`; the proof ledger records exit 0.
- Direct proof commit `cf9f130` (`test(agent): compare summarization system prompt`) adds the differential fixture, runtime adapter, and standalone direct-module type proof.
- Oracle output `/tmp/pi-rust-summarization-system-prompt-oracle.out` captures existing oracle typechecks plus the standalone direct-module exact-literal proof; the proof ledger records exit 0. `node --check oracle/summarization-system-prompt-typecheck.mjs` also passes.
- Formatting and diff checks pass; the proof ledger records exit 0. Rust, TypeScript, and JavaScript readability report no new findings; changed oracle dispatch was extracted while pre-existing `executeUtilityRequest` remains unchanged.
- Initial broad verification at `cf9f130`: workspace tests recorded 238 passed, 0 failed, and 2 ignored; all-target check, fmt, diff, and Node syntax passed; all-feature Clippy failed only on `clippy::unnecessary-map-or` in `crates/pi-agent-core/tests/summarization_system_prompt.rs`.
- Correction commit `08e105a` changes only `crates/pi-agent-core/tests/summarization_system_prompt.rs`, replacing behavior-preserving `map_or(true, ...)` with `is_none_or(...)`; no warning suppression or semantic change.
- Post-fix verification at `08e105a` reports `OVERALL: PASS`: workspace tests 238 passed, 0 failed, and 2 ignored; all-target check and all-feature Clippy with denied warnings exited 0; fmt and diff exited 0; source pin and worktree are exact and clean. Oracle/differential/typeproof results remain valid and the module-only export boundary is preserved.
- Post-fix artifacts `/tmp/pi-rust-summarization-system-prompt-post-fix-{workspace-tests,check,clippy,fmt,diff}.out` capture command output and explicit exit codes; the proof ledger records the corresponding results where applicable.
- Final workspace, static, readability, documentation, and clean-pin gates are complete. Inherited independent verification remains pending.

## Exclusions

`SUMMARIZATION_PROMPT`, prompt construction, compaction and branch orchestration, model calls, retries, serialization, token estimation, session behavior, the coding-agent duplicate, package/root exports, dynamic formatting, dependencies, and unrelated compaction APIs remain deferred.
