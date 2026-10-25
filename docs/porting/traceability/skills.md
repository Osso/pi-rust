# skills.ts traceability

Source: `packages/coding-agent/src/core/skills.ts` (487 lines, pinned revision in
`docs/porting/source-lock.json`). Rust: `crates/pi-coding-agent-core/src/core/skills.rs`.

## Exported symbols

| TS symbol | Rust item | Tests |
| --- | --- | --- |
| `SkillFrontmatter` | frontmatter read through `serde_json::Value` (`name`, `description`, `disable-model-invocation`; unknown keys ignored) | differential `unknown-field`, `disable-model-invocation` |
| `Skill` | `Skill` (`camelCase` serde) | differential `load_dir`/`load` compare every field |
| `LoadSkillsResult` | `LoadSkillsResult` | same |
| `loadSkillsFromDir` | `load_skills_from_dir` | differential 13 single-directory fixtures + focused tests |
| `formatSkillsForPrompt` | `format_skills_for_prompt` | differential `format` incl. XML escaping and hidden skills; `formats_visible_skills_as_xml` |
| `LoadSkillsOptions` | `LoadSkillsOptions` | differential `load` |
| `loadSkills` | `load_skills` (returns `Result` for `resolvePath` failures) | differential `load` (defaults, explicit dirs/files, missing path, non-markdown, user/project scope) |
| private `validateName` / `validateDescription` | `validate_name` / `validate_description` | differential fixture diagnostics; UTF-16 length like `String.length` |
| private `createSkillSourceInfo` | `create_skill_source_info` | differential compares `sourceInfo.scope/source/baseDir` for user/project/path/custom sources |
| private ignore-rule helpers | `prefix_ignore_pattern` / `add_ignore_rules` reusing `pi-agent-core` `IgnoreMatcher` | differential tree fixture with `.gitignore`, `node_modules`, dot directory |

## Branches covered by the differential

- Root `SKILL.md` wins and stops recursion; nested `SKILL.md` still found when no root skill.
- Direct root `.md` files load only when `includeRootFiles` is true.
- Missing directory returns empty; broken/unreadable entries are skipped.
- `.gitignore`/`.ignore`/`.fdignore` rules, `node_modules`, and dot-directories are skipped.
- Name validation: length, character set, leading/trailing hyphen, consecutive hyphens.
- Description required/whitespace-only/length.
- Name collision keeps the first skill and appends a `collision` diagnostic after all warnings.
- Symlinked duplicates are skipped by canonical path (same `canonicalizePath` fallback).

## Gaps / deviations

- YAML parse-error wording differs (`serde_yaml_ng` vs the Node `yaml` package). The differential
  excludes the invalid-YAML fixture; a focused test asserts one warning at the path that names a source
  line. See `frontmatter.md`.
- Non-string `name`/`description` frontmatter values are excluded by the typed Rust boundary; TypeScript
  would throw `description.trim is not a function` and report it as a warning.
- Windows separator conversion is implemented (`to_posix_path`) but not exercised on this host.

## Proof

- Focused: `cargo test -p pi-coding-agent --test skills` (5 passed).
- Differential: `cargo test -p pi-coding-agent --test differential_skills` (1 passed) via
  `oracle/coding-agent-skills-oracle.ts`, comparing skills and diagnostics in source order. Directory
  entries are locale-collated in Rust to match libuv `scandir` order used by Node `readdirSync`.

## Q18 common-pin native migration
Skill names/descriptions/paths/source metadata and load options retain JsString. Native ignore matcher receives original paths; XML prompt escaping retains raw units. Current-directory resolution errors propagate instead of selecting `/`. Directory entries are collected without dropping errors, then byte-sorted as libuv scandir; traversal exceptions preserve the frozen whole-directory catch. `load_skills_from_dir` now returns Result to expose source path-resolution exceptions. The TS-created raw skill directory fixture lives in `lossless_resources`; `skills-red.jsonl` is library-blocked, not runtime PASS. Isolated YAML parser restriction remains tracked in [frontmatter](frontmatter.md). Source-info.source is also native because caller-supplied skill source labels are arbitrary strings.

## Approved YAML gap (2026-10-22)

User decision: **Record gap**. Keep `serde_yaml_ng`; frontmatter YAML containing
unpaired UTF-16 units (literal or escaped) fails explicitly in
`parse_yaml_frontmatter`. No replacement, silent omission, or partial YAML. This
is an approved parity gap, not pending interim behavior. Native body text outside
YAML remains lossless.
