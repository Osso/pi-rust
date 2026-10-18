# system-prompt.ts traceability

Source: `packages/coding-agent/src/core/system-prompt.ts` (218 lines, pinned revision in
`docs/porting/source-lock.json`). Rust: `crates/pi-coding-agent/src/core/system_prompt.rs`.

| TS symbol | Rust item | Tests |
| --- | --- | --- |
| `BuildSystemPromptOptions` | `BuildSystemPromptOptions` (`Option` for omitted fields) | differential 19 option sets |
| inline `{ path, content }` context files | `ContextFile` | differential + focused |
| `buildSystemPrompt` | `build_system_prompt` | differential compares the full prompt; 6 focused tests |
| private `appendUserRulesSection` | `append_user_rules_section` | `wraps_rules_after_project_context_and_omits_empty_rules` |
| private guideline assembly | inline `guidelines_list`/`guidelines_set` | differential delegation/backgrounding/guideline cases |
| `DEFAULT_ACTIVE_TOOL_NAMES` (`tools/index.ts`) | `core::tools::DEFAULT_ACTIVE_TOOL_NAMES` | default-tools differential cases |

## Branches

- Custom prompt replaces the default body; empty custom prompt falls through to the default.
- Custom prompt appends `\n` after the cwd line; default prompt does not (byte-identical).
- Context files and rules are appended in both branches; empty rules content emits no tags.
- Skills are appended only when the `read` tool is selected (custom: any selected tool set that
  includes `read`; default: `hasRead`).
- Tool list shows only tools with snippets; no visible tools renders `(none)`.
- Guidelines dedupe in insertion order, trim prompt guidelines, and skip blank ones.
- Auto-background guidance requires a backgroundable tool (`bash`/`pyrun_eval`) plus `wait_agent`.
- `cwd` backslashes become `/`.

## Gaps

- `getReadmePath`/`getDocsPath`/`getExamplesPath` come from the Rust package-dir resolution
  (`PI_PACKAGE_DIR`/executable dir) versus the TypeScript module location. The differential normalizes
  only those three absolute path lines; everything else is byte-compared.
- Tool snippets are a `HashMap`; only lookup order is observable, and the source uses an object lookup.

## Proof

- Focused: `cargo test -p pi-coding-agent --test system_prompt` (6 passed).
- Differential: `cargo test -p pi-coding-agent --test differential_system_prompt` (1 passed) via
  `oracle/coding-agent-system-prompt-oracle.ts`.
