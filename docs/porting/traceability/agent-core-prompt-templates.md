# agent-core prompt templates — common pin

Frozen: `packages/agent-core/src/harness/prompt-templates.ts`, `4f9e6f8a4`.
Rust: `crates/pi-agent-core/src/harness/prompt_templates.rs`.

| Frozen lines / symbols | Branches | Proof |
|---|---|---|
| 30–61 `loadPromptTemplates` | missing/non-md inputs, explicit file, directory, symlink resolution and diagnostics | `prompt_template_loading` 7; `differential_harness_core` loader fixtures |
| 70–97 `loadSourcedPromptTemplates` | ordered results, attached provenance, optional mapper | same focused/differential tests; mapper callback remains typed Rust |
| 99–172 directory/file loaders | sorted direct children, frontmatter body/description, fallback first nonblank line, 60 UTF-16 units and ellipsis | same fixtures; `lossless_prompt_templates::description_slice_preserves_split_surrogate_against_frozen` proves a raw high surrogate at unit 59 |
| 174–203 `resolveKind` | canonical symlinks and missing targets | `prompt_template_loading`, loader differential |
| 205–226 frontmatter/basename | newline normalization, delimiters, YAML/body, basename | loader fixtures; YAML diagnostic wording is not exact (see below) |
| 229–254 `parseCommandArgs` | ASCII spaces/tabs only, quote removal, empty-token omission, unmatched quotes, literal non-delimiter whitespace | `prompt_templates` 2 parser tests; native unpaired-unit differential |
| 257–267 `substituteArgs`, `formatPromptTemplateInvocation` | positional/slice indices, frozen sequential passes, JS aggregate replacement dollar tokens, overflow, content-only format | `prompt_templates` 6 substitution/format tests; native unpaired-unit differential |

Q9 representation: `PromptTemplate.description`, `content`, argument vectors and helper results now use `JsString`. No replacement conversion at slicing or formatting. Existing JsRegex engine owns UTF-16 regex/replacement semantics.

RED: `lossless_prompt_templates` failed before production changes: Rust unit 59 = 65533; frozen = 55357. GREEN: 2 differential tests, plus existing 8 helper tests, 7 loader tests and retained loader/message oracle target. Logs: `/tmp/pi-rust-repin-delta-agent-core/prompt-{red,green}.log`.

Approved deviations (user decision relayed by parent 2026-10-22): YAML parse-error diagnostic text may differ from TS `yaml`, while success/failure must match. The preexisting differential masks only wording and proves occurrence. Lone-surrogate YAML escapes remain the separately approved gap in AGENTS.md. Typed Rust excludes invalid JS runtime input types; does not exclude human-text UTF-16. No unapproved loader gap remains.
