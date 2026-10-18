# Harness system-prompt traceability

Pinned source: `packages/agent/src/harness/system-prompt.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.
Rust: `crates/pi-agent-core/src/harness/system_prompt.rs`. Behavioral port of
`packages/agent/test/harness/system-prompt.test.ts`.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `system-prompt.ts:3-9` | Filter `disableModelInvocation === true`; preserve visible order and duplicates; return `""` when nothing is visible; omit content | `format_skills_for_system_prompt`; `tests/skill_formatting.rs` focused tests and differential fixtures `13061..=13062` |
| `system-prompt.ts:11-22` | Exact fixed preamble, two-space indentation, one `<skill>` block per visible skill, `name`/`description`/`location` only, LF joins with no trailing newline | Same function; focused exact-output test and differential fixtures `13061..=13064` |
| `system-prompt.ts:24-33` | Escape `&`, `<`, `>`, `"`, `'` sequentially so existing entities double-escape | Private `escape_xml`; focused entity-order test and differential `13063..=13064` |

## Proof

- Focused: `tests/skill_formatting.rs` (8 tests, shared with the skill invocation formatter).
- Differential: `oracle/truncate-oracle.ts` operation `skill_formatting`, fixtures `13061..=13064`.
- Gaps: none.
