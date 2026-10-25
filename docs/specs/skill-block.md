# Skill block parsing

This specification covers the typed Rust boundary for parsing a complete skill block from user-message text, ported from `packages/coding-agent/src/core/skill-block.ts:1-22`. The pinned package export is near `packages/coding-agent/src/index.ts:281`; the Rust crate-root export is proven separately from the direct-module type proof. Pinned differential/oracle proof covers the same representable input boundary.

## Rust API

`pi_coding_agent` exposes `#[derive(Debug)] pub struct ParsedSkillBlock` with public mutable `name`, `location`, `content`, and `user_message` fields, plus `pub fn parse_skill_block(text: &str) -> Option<ParsedSkillBlock>`. The type and function are re-exported by both the `core` module and crate root.

## What it must do

- [x] Expose `ParsedSkillBlock` at the coding-agent crate root and `core` module with public mutable fields in source order: `name`, `location`, `content`, and optional `user_message`.
- [x] Parse only a complete anchored `<skill name="..." location="...">\n...\n</skill>` block with nonempty quoted attributes and exact LF structural delimiters; when an earlier closing delimiter leaves an invalid remainder, continue to the earliest later candidate that completes the anchored grammar.
- [x] Preserve representable Unicode, NUL, attribute newlines, content newlines, and empty content accepted by the source pattern.
- [x] Accept an optional suffix only after exact `\n\n` and require suffix text before applying JavaScript trim semantics; trimmed empty text becomes `None`.
- [x] Preserve JavaScript trim characters, including U+FEFF, without trimming non-ECMAScript whitespace such as U+001C.

## How it works

- [Skill-block traceability](../porting/traceability/skill-block.md)

## Implementation inventory

- `crates/pi-coding-agent-data/src/core/skill_block.rs` — parsed value and pure parser.
- `crates/pi-coding-agent-core/src/core/mod.rs` — public `core` exports.
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
- [x] Focused final GREEN: `/tmp/pi-rust-parse-skill-block-backtracking-focused-green.out`, exit 0; 8 passed and 0 failed. Differential final GREEN: `/tmp/pi-rust-parse-skill-block-backtracking-differential-green.out`, exit 0; 1 passed and 0 failed. Oracle/typeproof: `/tmp/pi-rust-parse-skill-block-oracle.out`, exit 0.

## Final integration proof

- [x] Run `cargo test --workspace --all-targets`: 250 passed, 0 failed, and 2 ignored at code HEAD `7423d25`.
- [x] Run workspace all-target check and all-target/all-feature Clippy with `-D warnings`: both exit 0.
- [x] Run oracle/typeproof, formatting, Node syntax, and diff checks: all exit 0.
- [x] Complete Rust/TypeScript/JavaScript readability, documentation, dependency, and clean-pin audits.

## Known gaps

- [ ] Consumer integration and broader coding-agent harness behavior remain future slices.

## Out of scope

Invalid JavaScript runtime input values, lone UTF-16 surrogates, constructors, serialization, regex/dependency choices, and consumer orchestration remain outside this parser behavior slice.
