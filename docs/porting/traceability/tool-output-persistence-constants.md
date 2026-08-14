# Tool-output persistence constants traceability

Pinned source: `packages/coding-agent/src/core/session-tool-output.ts:19-20` at commit `532f7d12cd19235081bd4d6e425bde185652d86f`.

| TypeScript source | Observable contract | Rust mapping and focused proof |
|---|---|---|
| `session-tool-output.ts:19` | Export numeric literal `1048576`. | `pi_coding_agent::core::session_tool_output::MAX_PERSISTED_TOOL_RESULT_CONTENT_BYTES: usize = 1_048_576`; the crate root re-exports it and the focused test compares both paths. `usize` is a narrow exception because the source declaration is the exact numeric literal and all source uses are byte limits; it does not generalize unrestricted JavaScript `number` mappings. |
| `session-tool-output.ts:20` | Export exact string literal `"\\n[tool output truncated at 1 MiB]"`. | `pi_coding_agent::core::session_tool_output::TOOL_RESULT_TRUNCATION_MARKER: &str`; the crate root re-exports the same exact value and the focused test checks bytes, ASCII form, leading LF, and no trailing LF. |
| `index.ts:262-269` | Package index re-exports both constants. | Rust exposes the module and crate-root paths. `core` intentionally does not aggregate-re-export them. |
| Focused production boundary | Preserve constants without porting their consumers. | Rust adds no migration, truncation, UTF-8, serialization, filesystem, session, message, or content behavior and adds no dependency. |
| Differential request `13178` | Compare source-shaped values against the actual pinned TypeScript export. | Pending: one differential request will compare both exact constants. |
| Strict public-index proof | Preserve exact emitted declaration types and runtime values. | Pending: prove literal type `1048576` and literal string type/value through the pinned coding-agent package index; no broader API claim. |

## Focused proof status

- Focused RED: `/tmp/pi-rust-tool-output-constants-red.out` records the exact clean base `3e079e8`, exit `101`, with only missing module/root exports.
- Focused GREEN: the focused test records `1 passed, 0 failed` for both module and crate-root constants.
- Targeted Clippy, formatting, diff, and changed-file readability pass according to the current focused-production evidence.
- No differential/oracle/typeproof, broad workspace/static, final, or inherited verification is claimed yet.

## Rust boundary

The two constants are value-only exports. `usize` is intentionally limited to the byte-limit constant because the pinned TypeScript declaration is the exact numeric literal `1048576` and every source use is a byte limit. This is a documented narrow exception, not a change to the project rule that unrestricted JavaScript `number` fields map to `f64`.

No dependency change or deployment applies.

## Exclusions

Migration reports and functions, truncation, UTF-8/serialization behavior, filesystem/session/message/content handling, malformed recovery, backups, constructors, mutable identity, protocol/agent-core behavior, and unrelated APIs remain unported.
