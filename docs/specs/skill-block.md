# Skill block parsing

This specification covers the typed Rust boundary for parsing a complete skill block from user-message text, ported from `packages/coding-agent/src/core/skill-block.ts:1-22`. Pinned differential/oracle proof covers the same representable input boundary.

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
- `crates/pi-coding-agent/tests/differential_skill_block.rs` — persistent pinned differential runner.
- `crates/pi-coding-agent/tests/differential_skill_block/fixtures.rs` — 10 contiguous semantic fixture requests (`13138..=13147`).
- `oracle/skill-block-oracle.ts` — runtime adapter importing the pinned coding-agent public index.
- `oracle/skill-block-typecheck.mjs` — strict direct-module shape, mutability, and callable-result proof.

## Tests asserting this spec

- `crates/pi-coding-agent/tests/skill_block.rs`
- `crates/pi-coding-agent/tests/differential_skill_block.rs`
- `oracle/skill-block-typecheck.mjs`

## Differential/oracle proof

- [x] Compare 10 persistent requests (`13138..=13147`) covering valid records, mismatch cases, exact LF anchoring, first-quote boundaries, the earliest closing delimiter whose trailing grammar completes the anchored match, suffix grammar, JavaScript trim characters, U+001C preservation, Unicode, NUL, and full-input anchoring.
- [x] Keep runtime oracle normalization limited to JSON representation (`undefined` user message becomes `null`); parsing remains the pinned `parseSkillBlock` call.
- [x] Prove exact required mutable `ParsedSkillBlock` fields and `parseSkillBlock` return type with a direct-module TypeScript fixture; runtime differential import proves the package public export.
- [x] Regression-test regex-candidate backtracking when an earlier closing delimiter leaves an invalid remainder; request `13141` remains within the original fixture range.

## Known gaps (current cycle)

- [ ] Audit consumer integration and broader coding-agent harness behavior.

## Out of scope

Invalid JavaScript runtime input values, lone UTF-16 surrogates, constructors, serialization, regex/dependency choices, and consumer orchestration remain outside this parser behavior slice.
