# Agent-core message marker traceability

Pinned source: `packages/agent/src/harness/messages.ts:4-17` and package export `packages/agent/src/index.ts:76` at commit `532f7d12cd19235081bd4d6e425bde185652d86f`.

| TypeScript source | Observable contract | Rust proof or gap |
|---|---|---|
| `messages.ts:4-7` | Compaction prefix is the exact 96-byte LF-only literal ending after `<summary>\n` | `harness::messages::COMPACTION_SUMMARY_PREFIX`; focused exact-value, length, and framing tests; direct package-index request `13096` |
| `messages.ts:9-10` | Compaction suffix is the exact 11-byte literal `\n</summary>` | `COMPACTION_SUMMARY_SUFFIX`; focused leading-LF and exact-value proof; request `13096` |
| `messages.ts:12-15` | Branch prefix is the exact 89-byte LF-only literal ending after `<summary>\n` | `BRANCH_SUMMARY_PREFIX`; focused exact-value, length, and framing tests; request `13096` |
| `messages.ts:17` | Branch suffix is the exact 10-byte literal `</summary>` without a leading LF | `BRANCH_SUMMARY_SUFFIX`; focused no-leading-LF and exact-value proof; request `13096` |
| `index.ts:76` | `export * from "./harness/messages.ts"` exposes all four runtime constants at package root | Rust re-exports all four from `pi_agent_core::harness`; focused imports prove module and harness-root paths; request `13096` compares runtime exports and `public-index-typecheck.mjs` proves exact literal types |

## Rust boundary

- The constants are `pub const &str` values in `pi_agent_core::harness::messages` and are re-exported from `pi_agent_core::harness`.
- TypeScript infers exact string-literal types; Rust represents the same immutable compile-time values directly rather than adding a wrapper type.
- Values are ASCII and LF-only. No CR, trailing spaces, runtime interpolation, escaping, trimming, or normalization is part of the contract.

## Proof status

- Focused RED: `/tmp/pi-rust-summary-markers-red.out` records exit 101 with only the missing `harness::messages` module and harness-root constants.
- Focused GREEN: `/tmp/pi-rust-summary-markers-green.out` records exit 0 with 4 passed and 0 failed.
- Differential RED: `/tmp/pi-rust-summary-markers-differential-red.out` records all prior requests through `13095` passing before request `13096` reaches the unchanged oracle's unsupported operation and returns non-JSON.
- Differential GREEN: `/tmp/pi-rust-summary-markers-differential-green.out` records exit 0 with the persistent differential test passing request `13096`.
- Oracle/public-index typecheck: `/tmp/pi-rust-summary-markers-oracle.out` records `./run-tests.sh oracle` exit 0 with exact literal-type and package-export proof.

## Exclusions

Message interfaces, summary constructors, conversion, serialization, compaction/session orchestration, dynamic formatting, fallbacks, and dependencies remain deferred.
