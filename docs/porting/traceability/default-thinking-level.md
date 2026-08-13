# DEFAULT_THINKING_LEVEL traceability

Pinned source: `packages/coding-agent/src/core/defaults.ts:1-3` at commit `532f7d12cd19235081bd4d6e425bde185652d86f`.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `defaults.ts:1` | Import the agent-harness `ThinkingLevel` type. | `crates/pi-coding-agent/src/core/defaults.rs` imports `pi_agent_core::harness::ThinkingLevel` through the exact local dependency. |
| `defaults.ts:3` | Export `DEFAULT_THINKING_LEVEL` with the exact typed value `"medium"`. | `pub const DEFAULT_THINKING_LEVEL: ThinkingLevel = ThinkingLevel::Medium`; focused behavior proves typed equality, the exact literal, and Copy use. |
| Coding-agent package index | `defaults.ts` is not exported by the pinned coding-agent index. | Rust adds no crate-root or `core` re-export of the constant; only `pi_coding_agent::core::defaults::DEFAULT_THINKING_LEVEL` is public. |
| Differential request `13167` | Read the actual exported constant from the pinned direct module. | `differential_default_thinking_level` invokes `default-thinking-level-oracle.ts` with `packages/coding-agent/src/core/defaults.ts`; Rust compares the returned value to `DEFAULT_THINKING_LEVEL.as_str()`. |
| Standalone direct-module type/value proof | Preserve the source type identity and separately verify the exact runtime literal without a package-index claim. | `default-thinking-level-typecheck.mjs` imports `defaults.ts` and `packages/agent/src/types.ts` directly, proves `typeof DEFAULT_THINKING_LEVEL` is `ThinkingLevel`, then executes the fixture and requires `"medium"`. |

## Rust boundary

The direct module constant uses the existing agent-harness `ThinkingLevel` enum. This slice adds no agent-core, protocol, parser, serde, provider, resolver, session, or SDK behavior.

## Differential/oracle proof

- Committed revisions: production `96080f7`; differential/typeproof `9216184`. The exact local dependency and Cargo.lock update are in the production commit; the later proof commit changes no production Rust or dependency files.
- Focused RED: the original `/tmp/pi-rust-default-thinking-level-red.out` capture is zero bytes and is not evidence. `/tmp/pi-rust-default-thinking-level-red-reproduced.out` is the valid exact-parent reproduction at `a5b474b`, exit `101`, with only the missing `pi_agent_core` and `core::defaults` imports.
- Differential RED: `/tmp/pi-rust-default-thinking-level-differential-red.out` exits 101 before oracle support; the live test reaches the missing `oracle/default-thinking-level-oracle.ts` boundary and prior production behavior is not changed.
- GREEN: `/tmp/pi-rust-default-thinking-level-differential-green.out` exits 0 with one persistent direct-module request, `13167`, and 1 passed, 0 failed.
- Oracle/type/value proof: `/tmp/pi-rust-default-thinking-level-oracle.out` records `node oracle/default-thinking-level-typecheck.mjs` exit 0. The script is registered in `run-tests.sh oracle`; its fixture imports only the direct source modules and makes no package-index claim.
- Bounded gates: `/tmp/pi-rust-default-thinking-level-oracle-source-typecheck.out`, `/tmp/pi-rust-default-thinking-level-fmt.out`, `/tmp/pi-rust-default-thinking-level-diff.out`, `/tmp/pi-rust-default-thinking-level-node-check.out`, `/tmp/pi-rust-default-thinking-level-readability.out`, `/tmp/pi-rust-default-thinking-level-readability-ts-oracle.out`, and `/tmp/pi-rust-default-thinking-level-readability-js-typeproof.out` are nonempty and exit 0.
- Final integration at code/proof HEAD `9216184`: `/tmp/pi-rust-default-thinking-level-final-workspace-tests.out` records `266 passed, 0 failed, 2 ignored`; final workspace check, all-feature Clippy with `-D warnings`, oracle, formatting, Node syntax, and diff artifacts exit 0.
- The pinned source worktree is clean at `532f7d12cd19235081bd4d6e425bde185652d86f`. No deployment applies. Inherited independent verification remains pending.

## Exclusions

The pinned package-index omission remains intentional and unproved by this slice. No package-level default export, crate-root re-export, fallback, consumer wiring, CLI parser, provider behavior, protocol conversion, serialization, validation, resolver/session/default-selection behavior, broad integration proof, or inherited verification is included.
