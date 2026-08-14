# Thinking-level validator traceability

Pinned source: `packages/coding-agent/src/cli/args.ts:67-70` at commit `532f7d12cd19235081bd4d6e425bde185652d86f`.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| Private `VALID_THINKING_LEVELS` table | The validator recognizes exactly the eight agent-harness literals in source order and does not expose the table. | `crates/pi-coding-agent/src/cli/args.rs` stores a private `[ThinkingLevel; 8]` table and compares each value through `as_str`; focused tests cover valid and invalid strings. |
| `isValidThinkingLevel(level: string)` | Exact string membership predicate over the typed string boundary. | `pi_coding_agent::cli::args::is_valid_thinking_level(level: &str) -> bool`; no parser, enum conversion, or public table. |
| `level is ThinkingLevel` return predicate | TypeScript narrows the caller's variable after a true result. | Rust records the approved boundary as `bool`. The standalone proof imports `ThinkingLevel` from pinned `packages/agent/src/types.ts`, checks the exact predicate type and an `if`-branch narrowing assignment, and does not substitute `Option<ThinkingLevel>` or parser behavior. |
| `packages/coding-agent/src/index.ts:3` | The package index exports only `Args` and `parseArgs`; the validator is not package-exported. | No `cli` or crate-root re-export; only the direct module path is claimed. |
| Differential request `13177` | Compare the actual direct-module runtime result against the Rust predicate. | `thinking-level-validator-oracle.ts` dynamically imports `packages/coding-agent/src/cli/args.ts` and returns source-shaped `{value, valid}` records for all request strings; no package-index oracle claim. |

## Focused production proof

- Focused RED exits `101` at the missing function boundary only.
- Focused GREEN records `2 passed, 0 failed`.
- Targeted denied-warning Clippy, `cargo fmt --check`, `git diff --check`, and changed-file readability pass.

## Bounded differential/typeproof proof

- RED `/tmp/pi-rust-thinking-level-validator-differential-red.out` exits `101` at request `13177` with structured `{ok:false,error:{name:"Error",message:"unsupported operation: thinking_level_validator"}}`; it is not a startup, missing-module, or non-JSON failure.
- GREEN `/tmp/pi-rust-thinking-level-validator-differential-green.out` exits `0` with `1 passed, 0 failed`. One request contains `off`, `minimal`, `low`, `medium`, `high`, `xhigh`, `max`, `ultra`, then `""`, `OFF`, `Medium`, leading/trailing whitespace, `x-high`, `extreme`, `unsupported`, `é😀`, an embedded U+0000, and an embedded U+000A. The response shape is `{id, ok:true, output:{results:[{value, valid}]}}`.
- `/tmp/pi-rust-thinking-level-validator-oracle.out` and `/tmp/pi-rust-thinking-level-validator-aggregate-oracle.out` exit `0`. The standalone proof verifies source-lock/pinned HEAD, exact `(level: string) => level is ThinkingLevel`, caller narrowing, runtime valid/invalid cases, private `VALID_THINKING_LEVELS` non-export, and the pinned `highlight-js-lib-index.d.ts` root. `--skipLibCheck` matches pinned `tsconfig.base.json`; strict checking remains enabled for the generated fixture and imported source files.
- Node syntax, targeted denied-warning Clippy, formatting, diff, and Rust/TypeScript/JavaScript readability artifacts pass: `/tmp/pi-rust-thinking-level-validator-node-oracle.out`, `/tmp/pi-rust-thinking-level-validator-node-typeproof.out`, `/tmp/pi-rust-thinking-level-validator-clippy.out`, `/tmp/pi-rust-thinking-level-validator-fmt.out`, `/tmp/pi-rust-thinking-level-validator-diff.out`, and `/tmp/pi-rust-thinking-level-validator-readability.out`.

Broad workspace/check/Clippy, final integration, documentation closure, clean-pin, and inherited verification remain pending.

## Rust boundary

The function accepts `&str`, matching the approved Rust representable subset of the TypeScript `string` input. Invalid JavaScript runtime input types and caller-side narrowing effects are excluded. The existing exact local `pi-agent-core` dependency supplies `ThinkingLevel`; this slice adds no dependency and does not alter protocol or agent-core code.

## Exclusions

`Mode`, `Args`, `parseArgs`, diagnostics, help, dispatch, public valid-table exports, parser/enum conversion, `FromStr`, `Display`, serde, provider/model/default behavior, non-string runtime inputs, package/root exports, protocol or agent-core changes, dependencies, and unrelated APIs remain deferred.
