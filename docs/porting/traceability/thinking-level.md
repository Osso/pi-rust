# ThinkingLevel traceability

Pinned source: `packages/agent/src/types.ts:293-298` and package export `packages/agent/src/index.ts:145` at commit `532f7d12cd19235081bd4d6e425bde185652d86f`. The separate protocol schema is `packages/protocol/src/schemas.ts:26-35` and omits `"ultra"`.

| TypeScript source | Observable contract | Rust mapping and focused proof |
|---|---|---|
| `types.ts:293-298` | Closed agent-harness union with eight literals: `off`, `minimal`, `low`, `medium`, `high`, `xhigh`, `max`, `ultra` | `harness::types::ThinkingLevel::{Off, Minimal, Low, Medium, High, XHigh, Max, Ultra}` with `as_str`; `variants_return_exact_source_literals` |
| `types.ts:293-298` | Exact source-shaped value set; `ultra` remains distinct from `max` | `ultra_is_distinct_from_max` asserts distinct variants and literals |
| `index.ts:145` | Wildcard package export exposes the agent-harness type | `pi_agent_core::harness::ThinkingLevel`; `harness_root_reexports_the_same_enum` proves the requested Rust public path. No crate-root direct re-export is added. |
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
- `/tmp/pi-rust-agent-thinking-level-fmt.out` — `cargo fmt --all -- --check`, exit 0.
- `/tmp/pi-rust-agent-thinking-level-diff.out` — `git diff --check`, exit 0.
- `/tmp/pi-rust-agent-thinking-level-readability.out` — changed Rust files, 0 issues.

## Exclusions

No parser, formatter, serializer, validation, provider/model behavior, default constant, protocol conversion, package-index typeproof, differential/oracle request, broad suite, dependency, coding-agent API, or adjacent API is claimed by this slice.
