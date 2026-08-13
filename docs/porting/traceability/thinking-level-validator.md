# Thinking-level validator traceability

Pinned source: `packages/coding-agent/src/cli/args.ts:67-70` at commit `532f7d12cd19235081bd4d6e425bde185652d86f`.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| Private `VALID_THINKING_LEVELS` table | The validator recognizes exactly the eight agent-harness literals in source order and does not expose the table. | `crates/pi-coding-agent/src/cli/args.rs` stores a private `[ThinkingLevel; 8]` table and compares each value through `as_str`; focused tests cover valid and invalid strings. |
| `isValidThinkingLevel(level: string)` | Exact string membership predicate over the typed string boundary. | `pi_coding_agent::cli::args::is_valid_thinking_level(level: &str) -> bool`; no parser, enum conversion, or public table. |
| `level is ThinkingLevel` return predicate | TypeScript narrows the caller's variable after a true result. | Rust records the approved boundary as `bool`. This is a language-boundary discrepancy; `Option<ThinkingLevel>` and parser behavior are intentionally not substituted. Standalone TypeScript typeproof remains pending. |
| `packages/coding-agent/src/index.ts:3` | The package index exports only `Args` and `parseArgs`; the validator is not package-exported. | No `cli` or crate-root re-export; only the direct module path is claimed. |
| Differential request `13177` | Compare the actual direct-module runtime result against the Rust predicate. | Direct-module runtime oracle support is pending; no package-index oracle claim. |

## Focused production proof

- Focused RED exits `101` at the missing function boundary only.
- Focused GREEN records `2 passed, 0 failed`.
- Targeted denied-warning Clippy, `cargo fmt --check`, `git diff --check`, and changed-file readability pass.
- Differential, typeproof, broad, final, clean-pin, and inherited verification remain pending.

## Rust boundary

The function accepts `&str`, matching the approved Rust representable subset of the TypeScript `string` input. Invalid JavaScript runtime input types and caller-side narrowing effects are excluded. The existing exact local `pi-agent-core` dependency supplies `ThinkingLevel`; this slice adds no dependency and does not alter protocol or agent-core code.

## Exclusions

`Mode`, `Args`, `parseArgs`, diagnostics, help, dispatch, public valid-table exports, parser/enum conversion, `FromStr`, `Display`, serde, provider/model/default behavior, non-string runtime inputs, package/root exports, protocol or agent-core changes, dependencies, and unrelated APIs remain deferred.
