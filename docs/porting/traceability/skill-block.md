# Skill-block parsing traceability

Pinned source: `packages/coding-agent/src/core/skill-block.ts:1-22` at commit `532f7d12cd19235081bd4d6e425bde185652d86f`. The pinned package export is near `packages/coding-agent/src/index.ts:281`. Rust code/proof commit: `7423d25` (`7423d25497f52ad8f5cda6640f2f51f9793575ea`).

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `skill-block.ts:1-6` | Required mutable fields are `name`, `location`, `content`, and `userMessage` in source order. | `ParsedSkillBlock` uses public `String`, `String`, `String`, and `Option<String>` fields in the same order; focused tests and direct-module typeproof observe every field. |
| `skill-block.ts:11-12` | The regular expression is anchored, requires nonempty quoted attributes, uses exact LF structure, and captures content non-greedily; if a candidate closing delimiter leaves an invalid remainder, matching backtracks to the earliest later delimiter that permits the full anchored match. | `parse_skill_block` uses exact prefix/suffix checks, examines closing delimiters in order, and accepts the earliest candidate whose trailing grammar is valid; focused tests and persistent differential fixtures cover valid, minimal, multiline, malformed, quote, newline, anchoring, and backtracking cases. |
| `skill-block.ts:12-19` | The optional message requires `\n\n` plus at least one code unit, then uses JavaScript `.trim()` and maps empty output to `undefined`. | Rust requires a nonempty double-LF suffix, applies the ECMAScript trim set including U+FEFF/U+2028/U+2029, preserves U+001C, and maps empty output to `None`; focused tests and differential fixtures cover trimmed, empty, and nonempty suffixes. |
| `coding-agent/src/index.ts:281` | The package exports both `ParsedSkillBlock` and `parseSkillBlock`. | The Rust crate root re-exports `ParsedSkillBlock` and `parse_skill_block`; focused tests import the crate-root API, and the persistent runtime oracle imports the pinned package index. |

## Ported TypeScript tests

Both cases of `packages/coding-agent/test/skill-block.test.ts` are ported in `tests/skill_block.rs` (`parses_a_skill_block_with_trailing_user_message`, `returns_none_for_normal_message_text`).

## Rust boundary

Rust `&str` accepts only valid UTF-8 scalar strings, so JavaScript lone UTF-16 surrogates remain excluded. For representable strings, all parser delimiters are ASCII and the scalar boundary preserves the source's observable slices. No validation, normalization, serialization, constructor, or dependency behavior is claimed.

## Proof status

### Production phase

- Focused RED: `/tmp/pi-rust-parse-skill-block-red.out`, exit 101; compilation reached only unresolved root imports for the missing API.
- Focused initial GREEN: `/tmp/pi-rust-parse-skill-block-green.out`, exit 0; 7 passed and 0 failed.
- Final focused GREEN after candidate-backtracking coverage: `/tmp/pi-rust-parse-skill-block-backtracking-focused-green.out`, exit 0; 8 passed and 0 failed.

### Differential/oracle phase

- Fixture requests are exactly `13138..=13147` (10 contiguous persistent requests), sent in source order by `crates/pi-coding-agent/tests/differential_skill_block/mod.rs`.
- RED: `/tmp/pi-rust-parse-skill-block-differential-red.out`, exit 101; request `13138` reached the existing oracle and received the expected unsupported-operation non-JSON response before skill-block oracle support existed.
- GREEN: `/tmp/pi-rust-parse-skill-block-differential-green.out`, exit 0; 1 differential test passed, 0 failed. This remains the current differential evidence after the backtracking fixture was retained within request `13141`.
- Backtracking regression RED: focused `/tmp/pi-rust-parse-skill-block-backtracking-focused-red.out`, exit 101; the new test returned `None` instead of content `first\n</skill>\nsecond`. Differential `/tmp/pi-rust-parse-skill-block-backtracking-differential-red.out`, exit 101; request `13141` returned Rust `parsed: null` versus the pinned parsed content.
- Backtracking regression GREEN: focused `/tmp/pi-rust-parse-skill-block-backtracking-focused-green.out`, exit 0; 8 tests passed, 0 failed. Differential `/tmp/pi-rust-parse-skill-block-backtracking-differential-green.out`, exit 0; 1 test passed, 0 failed.
- Runtime adapter: `oracle/skill-block-oracle.ts` calls only the dynamically imported pinned package-index `parseSkillBlock`; its `undefined` message normalization is JSON transport only.
- Typeproof: `/tmp/pi-rust-parse-skill-block-oracle.out`, exit 0 via `./run-tests.sh oracle`; `oracle/skill-block-typecheck.mjs` directly imports the pinned `skill-block.ts` module to prove exact required fields, public mutation, and `ParsedSkillBlock | null` return type. The direct-module proof avoids unrelated package-index source-graph type errors; runtime GREEN proves the package-index export.

### Current focused evidence

- Focused behavioral proof: 8 passed, 0 failed in `/tmp/pi-rust-parse-skill-block-backtracking-focused-green.out`.
- Differential proof: 1 test passed, 0 failed in `/tmp/pi-rust-parse-skill-block-backtracking-differential-green.out`.
- Oracle/typeproof proof: `/tmp/pi-rust-parse-skill-block-oracle.out`, exit 0; runtime uses the pinned package index while exact field/mutability proof imports the direct module.

### Focused gates for this phase

- Formatting: `cargo fmt --all -- --check`, exit 0, captured in `/tmp/pi-rust-parse-skill-block-backtracking-fmt.out`.
- Diff check: `git diff --check`, exit 0, captured in `/tmp/pi-rust-parse-skill-block-backtracking-diff.out`.
- Rust readability: production, focused, and fixture audits each reported 0 issues in `/tmp/pi-rust-parse-skill-block-backtracking-readability-{production,focused,fixtures}.out`.
- Typeproof fixture syntax: `node --check oracle/skill-block-typecheck.mjs`, exit 0, captured in `/tmp/pi-rust-parse-skill-block-node.out`.
- Initial oracle adapter/typeproof TypeScript/JavaScript readability: `/tmp/pi-rust-parse-skill-block-readability/summary.md`; changed oracle functions remained below cognitive 15/cyclomatic 20 thresholds.

### Final integration gates

- Workspace tests: `/tmp/pi-rust-parse-skill-block-final-workspace-tests.out` captures `cargo test --workspace --all-targets` at code HEAD `7423d25` with 250 passed, 0 failed, and 2 ignored; exit 0.
- Static gates: `/tmp/pi-rust-parse-skill-block-final-check.out` captures `cargo check --workspace --all-targets`; `/tmp/pi-rust-parse-skill-block-final-clippy.out` captures all-target/all-feature Clippy with `-D warnings`; both exit 0.
- Oracle and syntax: `/tmp/pi-rust-parse-skill-block-final-oracle.out` captures `./run-tests.sh oracle`; `/tmp/pi-rust-parse-skill-block-final-node.out` captures the standalone typeproof syntax check; both exit 0.
- Formatting and diff: `/tmp/pi-rust-parse-skill-block-final-fmt.out` and `/tmp/pi-rust-parse-skill-block-final-diff.out` record exit 0. Later changes are documentation-only and do not invalidate executable proof.
- Pins and documentation: the pinned source worktree is clean at `532f7d12cd19235081bd4d6e425bde185652d86f`; README, dependency inventory, compatibility, dedicated spec, and traceability audits are consistent.
