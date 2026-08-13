# ThinkingLevel traceability

Pinned source: `packages/agent/src/types.ts:293-298` and package export `packages/agent/src/index.ts:145` at commit `532f7d12cd19235081bd4d6e425bde185652d86f`. The separate protocol schema is `packages/protocol/src/schemas.ts:26-35` and omits `"ultra"`.

| TypeScript source | Observable contract | Rust mapping and focused proof |
|---|---|---|
| `types.ts:293-298` | Closed agent-harness union with eight literals: `off`, `minimal`, `low`, `medium`, `high`, `xhigh`, `max`, `ultra` | `harness::types::ThinkingLevel::{Off, Minimal, Low, Medium, High, XHigh, Max, Ultra}` with `as_str`; `variants_return_exact_source_literals` |
| `types.ts:293-298` | Exact source-shaped value set; `ultra` remains distinct from `max` | `ultra_is_distinct_from_max` asserts distinct variants and literals |
| `index.ts:145` | Wildcard package export exposes the agent-harness type | `pi_agent_core::harness::ThinkingLevel`; `harness_root_reexports_the_same_enum` proves the requested Rust public path. No crate-root direct re-export is added. |
| Differential request `13166` | One source-shaped plain-value group compares all eight final literals in source order | `thinking_level::compare` sends `off`, `minimal`, `low`, `medium`, `high`, `xhigh`, `max`, `ultra`; Rust emits `as_str()` values and the pinned oracle echoes typed literals. |
| Pinned agent public-index typeproof | Exact erased union and invalid-literal rejection | `oracle/public-index-typecheck.mjs` imports `ThinkingLevel` from `packages/agent/src/index.ts`, proves all eight assignments, and rejects `""` and `"extreme"`; protocol typeproof/schema is untouched. |
| Rust boundary | Approved trait boundary for this closed value | Exact `Debug`, `Clone`, `Copy`, `PartialEq`, and `Eq` derives; `enum_values_support_equality_copy_clone_and_debug` exercises every variant, copying, generic cloning, equality, and Debug output |

## Rust boundary

- `ThinkingLevel` is a public closed enum with exactly eight variants.
- `as_str` is the only conversion API and is a `pub const fn` returning exact source literals.
- The enum lives in `harness::types` and is re-exported only from `harness`.
- The implementation does not import, alias, extend, or modify `pi_protocol::ThinkingLevel`.

## Proof artifacts

- `/tmp/pi-rust-agent-thinking-level-red.out` — valid missing-API RED, exit 101, exactly two unresolved imports.
- `/tmp/pi-rust-agent-thinking-level-green.out` — focused GREEN, 4 passed and 0 failed.
- `/tmp/pi-rust-agent-thinking-level-clippy.out` — targeted Clippy with denied warnings, exit 0.
- `/tmp/pi-rust-agent-thinking-level-readability.out` — production focused changed-Rust files, 0 issues.
- `/tmp/pi-rust-agent-thinking-level-differential-red.out` — valid unsupported-operation RED at request `13166`, exit 101 after prior requests pass.
- `/tmp/pi-rust-agent-thinking-level-differential-green.out` — initial persistent differential GREEN, 1 passed and 0 failed.
- `/tmp/pi-rust-agent-thinking-level-oracle.out` — pinned oracle/typeproof command, exit 0.
- `/tmp/pi-rust-agent-thinking-level-node-check.out` — public-index typeproof JavaScript syntax check, exit 0.
- `/tmp/pi-rust-agent-thinking-level-readability-current.out` — readability RED: the initial `cases` fixture builder was 34 body lines.
- The fixture corpus is now the named immutable `CASES` table; `/tmp/pi-rust-agent-thinking-level-differential-green-readability.out`, `/tmp/pi-rust-agent-thinking-level-differential-clippy.out`, `/tmp/pi-rust-agent-thinking-level-fmt-current.out`, `/tmp/pi-rust-agent-thinking-level-diff-current.out`, and `/tmp/pi-rust-agent-thinking-level-readability-current-green.out` record differential, denied-warning Clippy, formatting, diff, and readability exit 0 with request content/order unchanged.
- `/tmp/pi-rust-agent-thinking-level-ts-oracle-readability.out` and `/tmp/pi-rust-agent-thinking-level-js-typeproof-readability.out` record changed TypeScript/JavaScript readability commands exiting 0 without threshold violations.

## Exclusions

No parser, formatter, serializer, validation, provider/model behavior, default constant, protocol conversion, broad suite, dependency, coding-agent API, or adjacent API is claimed by this slice. The differential request and package-index proof cover only the agent-harness final literals; they do not extend the protocol type or model/provider behavior.