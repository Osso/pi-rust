# Harness prompt-template loading traceability

Pinned source: `packages/agent/src/harness/prompt-templates.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.
Rust: `crates/pi-agent-core/src/harness/prompt_templates.rs`. Behavioral port of
`packages/agent/test/harness/prompt-templates.test.ts`.

Argument helpers and the invocation formatter are documented in [resources](resources.md); diagnostic values in
[prompt-template diagnostics](prompt-template-diagnostics.md).

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `prompt-templates.ts:30-62` | `loadPromptTemplates(env, paths)`: missing roots skipped, non-`not_found` root errors produce `file_info_failed`, directories load non-recursively, explicit `.md` files load, other kinds skipped | `load_prompt_templates`; focused missing/non-markdown test and differential fixtures `14410..=14414` |
| `prompt-templates.ts:64-97` | `loadSourcedPromptTemplates` attaches an uninterpreted source, optional `mapPromptTemplate` | `load_sourced_prompt_templates` with `PromptTemplateMapper`; focused sourced tests and differential `14414` |
| `prompt-templates.ts:99-127` | Directory entries sort by `localeCompare`, only resolved files ending in lowercase `.md` load, `list_failed` on listing errors | `load_templates_from_dir`; focused uppercase-extension test and differential `14412` |
| `prompt-templates.ts:129-175` | File load: read/parse diagnostics; name strips a case-insensitive `.md` suffix; description comes from string frontmatter or the first non-blank body line truncated to 60 UTF-16 units plus `...` | `load_template_from_file` + `strip_markdown_extension` + `utf16_prefix`; focused first-line/empty/numeric cases and differential `14412` |
| `prompt-templates.ts:177-203` | Symlink resolution through `canonicalPath` + `fileInfo`, `not_found` silent | `resolve_kind`; focused symlinked-file test and differential `14411` |
| `prompt-templates.ts:205-215` | Frontmatter normalization, `---` framing, YAML parse, trimmed body, parse errors | Shared private `harness::frontmatter`; focused parse test and differential `14413` |

## Discrepancies

- `localeCompare` ordering uses the same ASCII approximation as the TUI autocomplete slice; mixed-case ordering is
  covered by differential fixtures.
- YAML tags resolve differently from the `yaml` package (documented in [skills](skills.md)); differential fixtures avoid
  tagged values.
- YAML parser error text differs; differential fixtures compare `parse_failed` occurrence, code and path only.
- `read_failed` requires an OS-level read failure and is not exercised differentially.

## Proof

- Focused: `tests/prompt_template_loading.rs` (7 tests).
- Differential: operations `prompt_templates_load`/`prompt_templates_load_sourced`, fixtures `14410..=14414`, comparing
  templates, names, descriptions, bodies, sources and diagnostics byte-for-byte.
