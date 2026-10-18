# Harness skills traceability

Pinned source: `packages/agent/src/harness/skills.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.
Rust: `crates/pi-agent-core/src/harness/skills.rs`. Behavioral port of `packages/agent/test/harness/skills.test.ts`.

Value types and the invocation/system-prompt formatters are documented in [resources](resources.md); this doc covers
loading, traversal, ignore rules, validation and diagnostics.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `skills.ts:5-7` | `MAX_NAME_LENGTH=64`, `MAX_DESCRIPTION_LENGTH=1024`, ignore files `.gitignore`, `.ignore`, `.fdignore` | Constants in `skills.rs`; focused length and ignore tests |
| `skills.ts:11-17` | Five diagnostic codes; severity is always `warning` | `SkillDiagnosticCode`/`SkillDiagnosticType` with exact literals |
| `skills.ts:19-26` | `SkillDiagnostic` fields | `SkillDiagnostic` (Debug/Clone/PartialEq/Eq) |
| `skills.ts:44-67` | `loadSkills(env, dirs)`: missing roots skipped, non-`not_found` root errors produce `file_info_failed`, only directories load, results accumulate across inputs | `load_skills`; focused missing/file-root test and differential multi-input fixture `14406` |
| `skills.ts:69-101` | `loadSourcedSkills` attaches an uninterpreted source to each skill and diagnostic, optional `mapSkill` | `load_sourced_skills` with `SkillMapper`; focused sourced tests and differential `14409` |
| `skills.ts:103-124` | Internal loader re-checks the directory, adds ignore rules, reports `list_failed` | `Loader::prepare_dir` |
| `skills.ts:126-143` | A `SKILL.md` file is loaded and terminates the directory scan; non-file/ignored entries continue scanning | `Loader::skill_file_action` + early return; focused short-circuit and differential `14407` |
| `skills.ts:145-176` | Remaining entries sort by `localeCompare`, skip dot names and `node_modules`, resolve symlinks, apply ignore rules (directory paths get a trailing slash), recurse into directories, and load direct root `.md` files only | `Loader::child_actions`; focused tests and differential `14401`, `14404`, `14406`, `14408` |
| `skills.ts:178-220` | Ignore files are read per directory; patterns are prefixed with the root-relative directory, comments/blank lines dropped, `!`/`\!`/leading `/` handled; read failures report `read_failed` | `Loader::add_ignore_rules` + `prefix_ignore_pattern`; differential ignore fixture `14403` |
| `skills.ts:222-245` | `prefixIgnorePattern` keeps the raw line (trim only for comment/blank detection) | `prefix_ignore_pattern` |
| `skills.ts:247-283` | Skill file load: read/parse failures, description/name validation diagnostics, skip when the description is missing/blank, `disable-model-invocation === true` | `Loader::load_skill_file`; focused validation/parse/tag tests; differential `14400`, `14404`, `14405` |
| `skills.ts:285-296` | Name validation: parent match, UTF-16 length, `[a-z0-9-]`, edge and consecutive hyphens | `validate_name`; focused message-order test |
| `skills.ts:298-309` | Description validation: required or over 1024 UTF-16 units | `validate_description`; focused overlong test |
| `skills.ts:311-326` | Frontmatter normalization, `---` framing, YAML parse, trimmed body, parse errors | Shared private `harness::frontmatter` (same behavior as the ported coding-agent module); focused CRLF/tag/parse tests and differential CRLF fixture |
| `skills.ts:328-357` | Symlink resolution through `canonicalPath` + `fileInfo`, `not_found` silent | `resolve_kind`; focused symlinked-directory test and differential `14402`, `14408` |
| `skills.ts:359-366` | `dirnameEnvPath` | `dirname_env_path` (formatter tests in `skill_formatting.rs`) |
| `skills.ts:368-375` | `relativeEnvPath` normalizes separators, trims trailing slashes, strips the root prefix | `relative_env_path`; exercised by every nested ignore fixture |

## Discrepancies

- The source uses the npm `ignore` package. The Rust port uses `ignore::gitignore::Gitignore::matched` with the same
  root, last-match-wins and negation semantics (verified by differential ignore fixtures including directory-only
  patterns, globs, negation and nested ignore files). `matched` is a single-path check, matching `ignores(path)`.
- The source sorts with `localeCompare`. The port uses the same ASCII approximation already approved for the TUI
  autocomplete slice (case-insensitive primary order, lowercase before uppercase). Mixed-case and punctuation ordering is
  covered by differential fixtures; full ICU tailoring for other scripts is not reproduced.
- YAML `!!binary` frontmatter: the JavaScript `yaml` package decodes it to bytes (so the string description is absent and
  the skill is skipped); `serde_yaml_ng` yields the scalar text, so the port loads the skill with the encoded payload as
  its description. Focused test `resolves_yaml_binary_tags_to_their_scalar_text`.
- YAML parser error text differs from the `yaml` package; differential fixtures compare `parse_failed` occurrence, code and
  path, not the message text.
- `read_failed` on a skill/ignore file requires an OS-level read failure; it is covered by focused branch code but not by a
  differential fixture (no portable unreadable file as the test user).

## Proof

- Focused: `tests/skill_loading.rs` (13 tests).
- Differential: `oracle/harness-core-oracle.ts` operations `skills_load`/`skills_load_sourced`, fixtures
  `14400..=14409`, comparing discovered skills, paths, flags, diagnostics and sources byte-for-byte.
