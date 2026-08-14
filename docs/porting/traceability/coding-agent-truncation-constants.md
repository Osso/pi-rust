# Coding-agent truncation constants traceability

Pinned source: `packages/coding-agent/src/core/tools/truncate.ts:11-13` at commit `532f7d12cd19235081bd4d6e425bde185652d86f`.

| TypeScript source | Observable contract | Rust mapping and focused proof |
|---|---|---|
| `truncate.ts:11` | Export numeric literal `2000` as `DEFAULT_MAX_LINES`. | `pi_coding_agent::core::tools::truncate::DEFAULT_MAX_LINES: usize`; value `2_000`. The coding-agent constant aliases `pi_agent_core::harness::utils::DEFAULT_MAX_LINES`. |
| `truncate.ts:12` | Export runtime value `10240` from `10 * 1024` as `DEFAULT_MAX_BYTES`; the emitted declaration type is broad `number`. | `pi_coding_agent::core::tools::truncate::DEFAULT_MAX_BYTES: usize`; value `10 * 1024`. The coding-agent constant aliases `pi_agent_core::harness::utils::DEFAULT_MAX_BYTES` under the approved typed truncation-limit boundary. |
| `truncate.ts:13` | Export numeric literal `500` as `GREP_MAX_LINE_LENGTH`. | `pi_coding_agent::core::tools::truncate::GREP_MAX_LINE_LENGTH: usize`; value `500`. The coding-agent constant aliases `pi_agent_core::harness::utils::GREP_MAX_LINE_LENGTH`. |
| `tools/index.ts:120-121` | Re-export only the two default constants from the tools index. | `pi_coding_agent::core::tools` re-exports `DEFAULT_MAX_LINES` and `DEFAULT_MAX_BYTES`; `GREP_MAX_LINE_LENGTH` remains direct-module-only. |
| `coding-agent/src/index.ts:323-324` | Re-export only the two default constants from the package root. | The crate root re-exports `DEFAULT_MAX_LINES` and `DEFAULT_MAX_BYTES`; `GREP_MAX_LINE_LENGTH` remains direct-module-only. |
| Approved typed boundary | These constants are non-negative truncation limits. | Rust uses `usize` for all three as an approved narrow boundary. This does not generalize JavaScript `number` mappings. |
| Differential request `13186` | One request reads the three direct-module runtime exports and compares normalized values. | Rust actual output uses only `pi_coding_agent::core::tools::truncate::{DEFAULT_MAX_LINES, DEFAULT_MAX_BYTES, GREP_MAX_LINE_LENGTH}`; the expected output comes independently from the pinned direct-module oracle. Differential GREEN: 1 passed, 0 failed. |
| Dedicated source proof | Real declarations, exact types, index aliases/omissions, valid assignments, invalid diagnostics, and direct/tools-index runtime imports are checked against pinned source. The package-root value is compiler-proven through its exact alias to the runtime-proven defining symbol; executing the real package root would initialize unrelated subsystems. | `coding-agent-truncation-constants-typecheck.mjs` passes: literal `2000`, broad `number`, literal `500`; five exact invalid diagnostics; direct-only GREP runtime presence. |

## Focused proof status

- Focused GREEN: `/tmp/pi-rust-coding-truncation-constants-focused-green.out` records `1 passed, 0 failed, 0 ignored`.
- Differential RED: `/tmp/pi-rust-coding-truncation-constants-differential-red.out` records request `13186`, Rust's three-value response, and the structured unsupported-operation oracle response; exit `101`.
- Differential GREEN: `/tmp/pi-rust-coding-truncation-constants-differential-green.out` records request `13186` with `1 passed, 0 failed, 0 ignored`.
- Dedicated compiler/runtime proof: `/tmp/pi-rust-coding-truncation-constants-typeproof.out` records exact source types, five invalid diagnostics, direct/tools-index runtime values, and compiler-verified package-root aliases.
- Oracle source typecheck, Node syntax, targeted Clippy, `cargo fmt --check`, and Rust/TypeScript/JavaScript readability audits pass. Aggregate oracle, workspace tests/check/Clippy, final diff/clean-pin audit, and inherited verification remain pending after the proof commit. No production Rust change, dependency change, push, or deployment was performed.

## Exclusions

No `TruncationResult`/`TruncationOptions` types, `truncateHead`, `truncateTail`, `truncateLine`, truncation algorithms, formatting behavior, tool behavior, filesystem/session behavior, dependencies, Cargo.lock, protocol behavior, or unrelated exports are ported by this slice. Differential/oracle/typeproof work for the three constants is complete.
