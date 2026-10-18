# Coding-agent initial message traceability

Pinned source: `packages/coding-agent/src/cli/initial-message.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Rust: `crates/pi-coding-agent/src/cli/initial_message.rs`.

| TypeScript source | Observable behavior | Rust proof |
|---|---|---|
| `initial-message.ts:15-23` | Part order: stdin, then `@file` text, then the first CLI message; the first message is shifted off `parsed.messages` | `tests/initial_message.rs` (4 tests, all three source cases plus empty/image cases) |
| `initial-message.ts:25-27` | Parts concatenate with no separator; no parts yields `undefined`; images are returned only when non-empty | Same tests |

## Semantics

- Rust takes `file_text: Option<&str>` and filters an empty string, matching the source's `if (fileText)` truthiness check.
- `initial_message` is `Option<String>` for the source `undefined`.

## Discrepancies

None found.
