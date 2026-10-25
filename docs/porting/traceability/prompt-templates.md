# prompt-templates.ts traceability

Source: `packages/coding-agent/src/core/prompt-templates.ts` (284 lines, pinned revision in
`docs/porting/source-lock.json`). Rust: `crates/pi-coding-agent-tools/src/core/prompt_templates.rs`.

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

## Current common-pin audit (2026-10-25)

Every export and branch above was compared with frozen 4f9e6f8a4. No new realistic-input delta remains. `cargo test -j3 -p pi-coding-agent --test suite_core_2 prompt_templates` passes the full differential (parse/substitute/expand/load); `suite_core_4 prompt_templates::` passes 3 native tests. Logs: `/tmp/ext-api-prompt-baseline.log`, `/tmp/ext-api-suite_core_4-prompt_templates.log`, revision 2683e5cbc. Old compiler-blocked notes below are historical, not current helper proof. Adversarial frontmatter types, filesystem races and numeric extremes are excluded by the current parity bar.

## Historical proof

- Focused: `cargo test -p pi-coding-agent --test prompt_templates` (3 passed).
- Differential: `cargo test -p pi-coding-agent --test differential_prompt_templates` (1 passed) via
  `oracle/coding-agent-prompt-templates-oracle.ts`, comparing templates in source order. Directory entries in that retired proof were locale-collated; Q18 replaces that incorrect construction with bytewise scandir order.

## Q18 common-pin migration
Template text/name/description/hint/file path and load options use JsString. Substitution uses native JsRegex capture ranges and native defaults/arguments. Source regex has numeric defaults only; the old `${ARGUMENTS:-...}` extension was removed to match the frozen expression. The file-description cap now retains native 60-unit slices instead of String::from_utf16_lossy. Filesystem operations alone encode paths; entries follow libuv bytewise scandir ordering, not locale comparison. Directory iteration errors are caught as a whole readdir operation. `lossless_resources::substitutions_keep_raw_template_defaults_and_arguments` is compiler-blocked (`resources-red.jsonl`), not runtime PASS. Old scalar corpus results above predate this change.

## Approved YAML gap (2026-10-22)

User decision: **Record gap**. Keep `serde_yaml_ng`; frontmatter YAML containing
unpaired UTF-16 units (literal or escaped) fails explicitly in
`parse_yaml_frontmatter`. No replacement, silent omission, or partial YAML. This
is an approved parity gap, not pending interim behavior. Native body text outside
YAML remains lossless.


## Verifier fix 2026-10-25

`expand_prompt_template` trimmed both ends of the argument tail; frozen `prompt-templates.ts:275-280` (`\s+([\s\S]*)`) drops only leading whitespace. `/greet "hello ` with `[$1]` now yields `[hello ]` like frozen. Native regression `prompt_templates::keeps_trailing_whitespace_inside_unclosed_quoted_argument` (RED `[hello]` → GREEN).
