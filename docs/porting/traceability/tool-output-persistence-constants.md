# Tool-output persistence constants traceability

Pinned source: `packages/coding-agent/src/core/session-tool-output.ts:19-20` at commit `532f7d12cd19235081bd4d6e425bde185652d86f`.

| TypeScript source | Observable contract | Rust mapping and focused proof |
|---|---|---|
| `session-tool-output.ts:19` | Export numeric literal `1048576`. | `pi_coding_agent::core::session_tool_output::MAX_PERSISTED_TOOL_RESULT_CONTENT_BYTES: usize = 1_048_576`; the crate root re-exports it and the focused test compares both paths. `usize` is a narrow exception because the source declaration is the exact numeric literal and all source uses are byte limits; it does not generalize unrestricted JavaScript `number` mappings. |
| `session-tool-output.ts:20` | Export exact string literal `"\\n[tool output truncated at 1 MiB]"`. | `pi_coding_agent::core::session_tool_output::TOOL_RESULT_TRUNCATION_MARKER: &str`; the crate root re-exports the same exact value and the focused test checks bytes, ASCII form, leading LF, and no trailing LF. |
| `index.ts:262-269` | Package index re-exports both constants. | Rust exposes the module and crate-root paths. `core` intentionally does not aggregate-re-export them. |
| Focused production boundary | Preserve constants without porting their consumers. | Rust adds no migration, truncation, UTF-8, serialization, filesystem, session, message, or content behavior and adds no dependency. |
| Differential request `13178` | Compare source-shaped values against the actual pinned TypeScript export. | The dedicated test sends exactly one request with operation `tool_output_persistence_constants`; the Rust module/root values match the direct-module oracle. |
| Strict public-index proof | Preserve exact emitted declaration types and runtime values. | The pinned TypeScript compiler API loads the real `tsconfig.json`, roots the real coding-agent index plus pinned highlight declaration, checks only the real index and defining-module diagnostics, verifies both public aliases resolve to the defining module, and proves exact number/string literal types. A separate pinned `tsx` runtime probe imports the real package index and passes with `1048576` and `"\\n[tool output truncated at 1 MiB]"`; no broader API claim. |

## Focused proof status

- Focused RED: `/tmp/pi-rust-tool-output-constants-red.out` records the exact clean base `3e079e8`, exit `101`, with only missing module/root exports.
- Focused GREEN: the focused test records `1 passed, 0 failed` for both module and crate-root constants.
- Targeted Clippy, formatting, diff, direct oracle source typecheck, Node syntax, and changed Rust/TypeScript/JavaScript readability artifacts pass.
- Differential RED: `/tmp/pi-rust-tool-output-constants-differential-red.out` records exit `101` after the oracle returns exact structured `{name: "Error", message: "unsupported operation: tool_output_persistence_constants"}`.
- Differential GREEN: `/tmp/pi-rust-tool-output-constants-differential-green.out` records request `13178`, `1 passed, 0 failed`.
- Runtime package-index proof: `/tmp/pi-rust-tool-output-constants-typeproof-current.out` passes with compiler version `5.9.3`, zero scoped diagnostics, and both exact values; `/tmp/pi-rust-tool-output-constants-oracle-current.out` passes through `./run-tests.sh oracle`.
- Symbol-scoped declaration proof uses the real pinned index and defining module, compiler flags/literal values, real-index alias declarations, inherited strict/`skipLibCheck` options, and no synthetic/fake index, copied declaration, untracked dist, or suppression.
## Final integration proof

- Code/proof HEAD: `66e30fa`; focused production commit: `1a8077a`.
- `/tmp/pi-rust-tool-output-constants-final-workspace-tests.out` records `cargo test --workspace --all-targets` exiting 0 with `278 passed, 0 failed, 2 ignored`.
- `/tmp/pi-rust-tool-output-constants-final-check.out` and `/tmp/pi-rust-tool-output-constants-final-clippy.out` record workspace check and all-target/all-feature Clippy with `-D warnings` exiting 0.
- `/tmp/pi-rust-tool-output-constants-final-oracle.out`, `/tmp/pi-rust-tool-output-constants-final-typeproof.out`, both `/tmp/pi-rust-tool-output-constants-final-node-*.out` artifacts, `/tmp/pi-rust-tool-output-constants-final-fmt.out`, and `/tmp/pi-rust-tool-output-constants-final-diff.out` exit 0 at `66e30fa`.
- `/tmp/pi-rust-tool-output-constants-final-readability.out` records the final changed Rust/TypeScript/JavaScript readability audit passing without suppressions or new violations.
- Documentation and dependency/lock audits pass; `3e079e8..66e30fa` changes no Cargo manifest or lockfile. No deployment applies.
- The Rust worktree is clean at `66e30fa`; the pinned source worktree is clean at `532f7d12cd19235081bd4d6e425bde185652d86f`.
- Inherited independent verification remains pending.

## Rust boundary

The two constants are value-only exports. `usize` is intentionally limited to the byte-limit constant because the pinned TypeScript declaration is the exact numeric literal `1048576` and every source use is a byte limit. This is a documented narrow exception, not a change to the project rule that unrestricted JavaScript `number` fields map to `f64`.

No dependency change or deployment applies.

## Exclusions

Migration reports and functions, truncation, UTF-8/serialization behavior, filesystem/session/message/content handling, malformed recovery, backups, constructors, mutable identity, protocol/agent-core behavior, and unrelated APIs remain unported.
