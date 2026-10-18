# prompt-templates.ts traceability

Source: `packages/coding-agent/src/core/prompt-templates.ts` (285 lines, pinned revision in
`docs/porting/source-lock.json`). Rust: `crates/pi-coding-agent/src/core/prompt_templates.rs`.

## Exported symbols

| TS symbol | Rust item | Tests |
| --- | --- | --- |
| `PromptTemplate` | `PromptTemplate` (`camelCase` serde, optional `argumentHint`) | differential `load` compares every field |
| `parseCommandArgs` | `parse_command_args` | differential `parse_args` (16 cases incl. quotes, tabs, newlines, unicode, literal backslashes) |
| `substituteArgs` | `substitute_args` | differential `substitute` (62 cases: `$N`, `$@`, `$ARGUMENTS`, defaults, slices, no recursion) |
| `LoadPromptTemplatesOptions` | `LoadPromptTemplatesOptions` | differential `load` |
| `loadPromptTemplates` | `load_prompt_templates` (returns `Result` for `resolvePath` failures) | differential `load` (global, project, explicit dir, explicit file, missing path, non-markdown) |
| `expandPromptTemplate` | `expand_prompt_template` | differential `expand` (matching, missing, newline separator, non-command) |
| private `loadTemplateFromFile` | `load_template_from_file` | differential `load` (frontmatter description, first-line fallback, 60-unit truncation, argument-hint) |
| private `loadTemplatesFromDir` | `load_templates_from_dir` | differential `load` |

## Branches

- Empty `argument-hint` and missing `argument-hint` both produce `None`.
- Description falls back to the first non-empty body line, truncated to 60 UTF-16 units plus `...`.
- `isUnderPath` chooses user/project source metadata; explicit paths are `temporary` with `baseDir`
  equal to the file's parent (or the directory itself for directory inputs).
- Missing explicit paths and non-`.md` files are skipped; read/parse failures drop the template.
- Substitution never re-scans replacement text, matching the single-pass `String.replace` callback.
- `[0-9]` replaces JavaScript `\d`; `is_js_whitespace` matches the ECMAScript whitespace set.

## Gaps

- Non-string frontmatter `description`/`argument-hint` values are excluded by the typed boundary; the
  TypeScript runtime would pass them through or throw on `.trim()`.
- `$` end-of-input semantics match non-multiline JavaScript.

## Proof

- Focused: `cargo test -p pi-coding-agent --test prompt_templates` (3 passed).
- Differential: `cargo test -p pi-coding-agent --test differential_prompt_templates` (1 passed) via
  `oracle/coding-agent-prompt-templates-oracle.ts`, comparing templates in source order. Directory entries
  are locale-collated in Rust to match libuv `scandir` order used by Node `readdirSync`.
