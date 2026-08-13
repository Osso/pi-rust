# Skill-block parsing traceability

Pinned source: `packages/coding-agent/src/core/skill-block.ts` at commit `532f7d12cd19235081bd4d6e425bde185652d86f`.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `skill-block.ts:1-6` | Required mutable fields are `name`, `location`, `content`, and `userMessage` in source order. | `ParsedSkillBlock` uses public `String`, `String`, `String`, and `Option<String>` fields in the same order; focused tests observe every field. |
| `skill-block.ts:11-12` | The regular expression is anchored, requires nonempty quoted attributes, uses exact LF structure, and captures content non-greedily through the first `\n</skill>`. | `parse_skill_block` uses exact prefix/suffix checks and the first closing delimiter; focused tests cover valid, minimal, multiline, malformed, quote, newline, and anchoring cases. |
| `skill-block.ts:12-19` | The optional message requires `\n\n` plus at least one code unit, then uses JavaScript `.trim()` and maps empty output to `undefined`. | Rust requires a nonempty double-LF suffix, applies the ECMAScript trim set including U+FEFF, preserves U+001C, and maps empty output to `None`; focused tests cover trimmed, empty, and nonempty suffixes. |
| `coding-agent/src/index.ts:281` | The package exports both `ParsedSkillBlock` and `parseSkillBlock`. | The Rust crate root re-exports `ParsedSkillBlock` and `parse_skill_block`; focused tests import the crate-root API. |

## Rust boundary

Rust `&str` accepts only valid UTF-8 scalar strings, so JavaScript lone UTF-16 surrogates remain excluded. For representable strings, all parser delimiters are ASCII and the scalar boundary preserves the source's observable slices. No validation, normalization, serialization, constructor, or dependency behavior is claimed.

## Production-phase proof

- Focused RED: `/tmp/pi-rust-parse-skill-block-red.out`, exit 101; compilation reached only unresolved root imports for the missing API.
- Focused GREEN: `/tmp/pi-rust-parse-skill-block-green.out`, exit 0; 7 passed and 0 failed.
- Formatting: `/tmp/pi-rust-parse-skill-block-fmt.out`, `cargo fmt --all -- --check`, exit 0.
- Readability: `/tmp/pi-rust-parse-skill-block-readability.out`; all four changed Rust files reported 0 issues.
- Differential/oracle proof is intentionally not present in this production phase.
