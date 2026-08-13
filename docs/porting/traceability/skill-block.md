# Skill-block parsing traceability

Pinned source: `packages/coding-agent/src/core/skill-block.ts` at commit `532f7d12cd19235081bd4d6e425bde185652d86f`.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `skill-block.ts:1-6` | Required mutable fields are `name`, `location`, `content`, and `userMessage` in source order. | `ParsedSkillBlock` uses public `String`, `String`, `String`, and `Option<String>` fields in the same order; focused tests and direct-module typeproof observe every field. |
| `skill-block.ts:11-12` | The regular expression is anchored, requires nonempty quoted attributes, uses exact LF structure, and captures content non-greedily; if a candidate closing delimiter leaves an invalid remainder, matching backtracks to the earliest later delimiter that permits the full anchored match. | `parse_skill_block` uses exact prefix/suffix checks, examines closing delimiters in order, and accepts the earliest candidate whose trailing grammar is valid; focused tests and persistent differential fixtures cover valid, minimal, multiline, malformed, quote, newline, anchoring, and backtracking cases. |
| `skill-block.ts:12-19` | The optional message requires `\n\n` plus at least one code unit, then uses JavaScript `.trim()` and maps empty output to `undefined`. | Rust requires a nonempty double-LF suffix, applies the ECMAScript trim set including U+FEFF/U+2028/U+2029, preserves U+001C, and maps empty output to `None`; focused tests and differential fixtures cover trimmed, empty, and nonempty suffixes. |
| `coding-agent/src/index.ts:281` | The package exports both `ParsedSkillBlock` and `parseSkillBlock`. | The Rust crate root re-exports `ParsedSkillBlock` and `parse_skill_block`; focused tests import the crate-root API, and the persistent runtime oracle imports the pinned package index. |

## Rust boundary

Rust `&str` accepts only valid UTF-8 scalar strings, so JavaScript lone UTF-16 surrogates remain excluded. For representable strings, all parser delimiters are ASCII and the scalar boundary preserves the source's observable slices. No validation, normalization, serialization, constructor, or dependency behavior is claimed.

## Proof status

### Production phase

- Focused RED: `/tmp/pi-rust-parse-skill-block-red.out`, exit 101; compilation reached only unresolved root imports for the missing API.
- Focused GREEN: `/tmp/pi-rust-parse-skill-block-green.out`, exit 0; 7 passed and 0 failed.

### Differential/oracle phase

- Fixture requests are exactly `13138..=13147` (10 contiguous persistent requests), sent in source order by `crates/pi-coding-agent/tests/differential_skill_block/mod.rs`.
- RED: `/tmp/pi-rust-parse-skill-block-differential-red.out`, exit 101; request `13138` reached the existing oracle and received the expected unsupported-operation non-JSON response before skill-block oracle support existed.
- GREEN: `/tmp/pi-rust-parse-skill-block-differential-green.out`, exit 0; 1 differential test passed, 0 failed.
- Backtracking regression RED: focused `/tmp/pi-rust-parse-skill-block-backtracking-focused-red.out`, exit 101; the new test returned `None` instead of content `first\n</skill>\nsecond`. Differential `/tmp/pi-rust-parse-skill-block-backtracking-differential-red.out`, exit 101; request `13141` returned Rust `parsed: null` versus the pinned parsed content.
- Backtracking regression GREEN: focused `/tmp/pi-rust-parse-skill-block-backtracking-focused-green.out`, exit 0; 8 tests passed, 0 failed. Differential `/tmp/pi-rust-parse-skill-block-backtracking-differential-green.out`, exit 0; 1 test passed, 0 failed.
- Runtime adapter: `oracle/skill-block-oracle.ts` calls only the dynamically imported pinned package-index `parseSkillBlock`; its `undefined` message normalization is JSON transport only.
- Typeproof: `/tmp/pi-rust-parse-skill-block-oracle.out`, exit 0 via `./run-tests.sh oracle`; `oracle/skill-block-typecheck.mjs` directly imports the pinned `skill-block.ts` module to prove exact required fields, public mutation, and `ParsedSkillBlock | null` return type. The direct-module proof avoids unrelated package-index source-graph type errors; runtime GREEN proves the package-index export.

### Focused gates for this phase

- Formatting: `cargo fmt --all -- --check`, exit 0, captured in `/tmp/pi-rust-parse-skill-block-backtracking-fmt.out`.
- Diff check: `git diff --check`, exit 0, captured in `/tmp/pi-rust-parse-skill-block-backtracking-diff.out`.
- Rust readability: production, focused, and fixture audits each reported 0 issues in `/tmp/pi-rust-parse-skill-block-backtracking-readability-{production,focused,fixtures}.out`.
- Typeproof fixture syntax: `node --check oracle/skill-block-typecheck.mjs`, exit 0, captured in `/tmp/pi-rust-parse-skill-block-node.out`.
- Initial oracle adapter/typeproof TypeScript/JavaScript readability: `/tmp/pi-rust-parse-skill-block-readability/summary.md`; changed oracle functions remained below cognitive 15/cyclomatic 20 thresholds.

No workspace tests, workspace check, Clippy, deployment, or inherited final-verifier claims are made by this phase.
