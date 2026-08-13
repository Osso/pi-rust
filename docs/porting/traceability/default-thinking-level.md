# DEFAULT_THINKING_LEVEL traceability

Pinned source: `packages/coding-agent/src/core/defaults.ts:1-3` at commit `532f7d12cd19235081bd4d6e425bde185652d86f`.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `defaults.ts:1` | Import the agent-harness `ThinkingLevel` type. | `crates/pi-coding-agent/src/core/defaults.rs` imports `pi_agent_core::harness::ThinkingLevel` through the exact local dependency. |
| `defaults.ts:3` | Export `DEFAULT_THINKING_LEVEL` with the exact typed value `"medium"`. | `pub const DEFAULT_THINKING_LEVEL: ThinkingLevel = ThinkingLevel::Medium`; focused behavior proves typed equality, the exact literal, and Copy use. |
| Coding-agent package index | `defaults.ts` is not exported by the pinned coding-agent index. | Rust adds no crate-root or `core` re-export of the constant; only `pi_coding_agent::core::defaults::DEFAULT_THINKING_LEVEL` is public. |

## Rust boundary

The direct module constant uses the existing agent-harness `ThinkingLevel` enum. This slice adds no agent-core, protocol, parser, serde, provider, resolver, session, or SDK behavior.

## Focused proof

- RED evidence correction: the original `/tmp/pi-rust-default-thinking-level-red.out` capture is zero bytes and is not used as proof. `/tmp/pi-rust-default-thinking-level-red-reproduced.out` uses exact pre-production revision `a5b474b`, injects the focused test, and records exit 101 with exactly the missing `pi_agent_core` dependency and `core::defaults` module diagnostics.
- GREEN: `/tmp/pi-rust-default-thinking-level-green.out` exits 0 with 1 passed and 0 failed.
- Self-describing targeted Clippy, package check, formatting, and diff artifacts record command, base revision, current production working-tree scope, and exit 0: `/tmp/pi-rust-default-thinking-level-clippy-proof.out`, `/tmp/pi-rust-default-thinking-level-cargo-check-proof.out`, `/tmp/pi-rust-default-thinking-level-fmt-proof.out`, and `/tmp/pi-rust-default-thinking-level-diff-proof.out`.
- Rust readability: `defaults.rs`, `core/mod.rs`, and `tests/defaults.rs` report no cognitive, cyclomatic, length, nesting, or suppression findings.

No differential/oracle or broad-suite evidence is claimed in this production/focused phase.

## Exclusions

The pinned package-index omission remains intentional. No package-level default export, crate-root re-export, fallback, consumer wiring, CLI parser, provider behavior, protocol conversion, serialization, validation, differential fixture, oracle adapter, or broad integration proof is included.
