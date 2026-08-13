# Skill block parsing

This specification covers the typed Rust boundary for parsing a complete skill block from user-message text, ported from `packages/coding-agent/src/core/skill-block.ts:1-22`. Differential/oracle proof is a later phase.

## What it must do

- [x] Expose `ParsedSkillBlock` at the coding-agent crate root and `core` module with public mutable fields in source order: `name`, `location`, `content`, and optional `user_message`.
- [x] Parse only a complete anchored `<skill name="..." location="...">\n...\n</skill>` block with nonempty quoted attributes and exact LF structural delimiters.
- [x] Preserve representable Unicode, NUL, attribute newlines, content newlines, and empty content accepted by the source pattern.
- [x] Accept an optional suffix only after exact `\n\n` and require suffix text before applying JavaScript trim semantics; trimmed empty text becomes `None`.
- [x] Preserve JavaScript trim characters, including U+FEFF, without trimming non-ECMAScript whitespace such as U+001C.

## How it works

- [Skill-block traceability](../porting/traceability/skill-block.md)

## Implementation inventory

- `crates/pi-coding-agent/src/core/skill_block.rs` — parsed value and pure parser.
- `crates/pi-coding-agent/src/core/mod.rs` — public `core` exports.
- `crates/pi-coding-agent/src/lib.rs` — crate-root public exports.
- `crates/pi-coding-agent/tests/skill_block.rs` — focused behavioral coverage.

## Tests asserting this spec

- `crates/pi-coding-agent/tests/skill_block.rs`

## Known gaps (current cycle)

- [ ] Add pinned TypeScript differential/oracle proof.
- [ ] Audit consumer integration and broader coding-agent harness behavior.

## Out of scope

Invalid JavaScript runtime input values, lone UTF-16 surrogates, constructors, serialization, regex/dependency choices, consumer orchestration, and differential/oracle infrastructure are outside this production phase.
