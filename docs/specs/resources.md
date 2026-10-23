# Agent-core skill and prompt resources

Agent-core exposes the dependency-free `Skill`, `PromptTemplate`, and concrete `AgentHarnessResources` value contracts from `packages/agent/src/harness/types.ts`, plus pure prompt-template and Skill formatting helpers; pinned public-index and direct differential proofs are complete for the values and both formatter slices, while loading and harness integration remain later slices. Prompt-template diagnostic values have a dedicated [specification](prompt-template-diagnostics.md) and [traceability](../porting/traceability/prompt-template-diagnostics.md). See the [resource traceability](../porting/traceability/resources.md) for source mappings.

## What it must do

### Skill

- [x] Expose mutable public `name`, `description`, `content`, `file_path`, and optional `disable_model_invocation` fields.
- [x] Preserve `None`, `Some(false)`, and `Some(true)` for `disable_model_invocation`.
- [x] Preserve empty, Unicode, supplementary, combining, embedded-NUL, and punctuation strings without normalization.

### Prompt template

- [x] Expose mutable public `name`, optional `description`, and `content` fields.
- [x] Preserve absent, empty, and non-empty descriptions.
- [x] Preserve empty, Unicode, supplementary, combining, embedded-NUL, and punctuation strings without normalization.

### Prompt-template argument helpers

- [x] Parse arguments using only ASCII space/tab delimiters; remove matching single/double quotes; preserve unmatched quotes, literal backslashes, adjacent fragments, non-delimiter whitespace, Unicode scalars, combining characters, NUL, and punctuation; omit empty quoted arguments.
- [x] Substitute positional `$N`, slice `${@:N}`/`${@:N:L}`, `$ARGUMENTS`, and `$@` passes in exact source order with one-based indices, leading-zero handling, and later-pass rescanning. Bounded decimal scanning maps huge positions/starts to missing or empty results and huge lengths to the argument-array end, matching JavaScript for representable array lengths.
- [x] Preserve JavaScript replacement-string semantics for aggregate passes, including `$$`, `$&`, `$```, `$'`, nonexistent captures, and original-input prefix/suffix behavior across multiple matches.
- [x] Format only `PromptTemplate.content`; callers pass `&[]` for the TypeScript default empty argument list.
- [x] Compare all three prompt-template helpers directly with the pinned source through 15 persistent differential fixtures (`13046..=13060`).

### Skill formatting

- [x] Format visible skills with exact fixed LF output, preserving order and duplicates while hiding only `Some(true)` model-invocation flags.
- [x] XML-escape only visible skill name, description, and file path fields through the five source-ordered entity replacements; preserve content exclusion and double escaping.
- [x] Format skill invocations with raw name, path, content, and optional instructions; omit absent/empty instructions and append nonempty instructions with exactly two LF characters.
- [x] Preserve lexical slash/backslash dirname behavior, including duplicate separators, Windows-like prefixes, and JavaScript UTF-16 drive-prefix indexing.

### Resource collection

- [x] Expose mutable public optional `prompt_templates` and `skills` arrays containing the concrete value types.
- [x] Preserve `None` versus `Some(vec![])`, array order, duplicate elements, nested mutation, and array replacement.
- [x] Export all three values from `pi_agent_core::harness` with `Debug` support and no additional traits or constructors.
- [x] Prove exact erased source structures, mutable fields, explicit `undefined` acceptance, and generic subtype capability through the pinned public index.
- [x] Compare plain mutable records for absent/undefined options, booleans, strings, arrays, order, and duplicates in 12 pinned differential fixtures (`13024..=13035`).

## How it works

- [Resource traceability](../porting/traceability/resources.md)

## Implementation inventory

- `crates/pi-agent-core/src/harness/types.rs` — resource value definitions.
- `crates/pi-agent-core/src/harness/prompt_templates.rs` — pure prompt-template argument helpers, exported only through `harness::prompt_templates`.
- `crates/pi-agent-core/src/harness/system_prompt.rs` — pure visible-Skill system-prompt formatting, exported only through `harness::system_prompt`.
- `crates/pi-agent-core/src/harness/skills.rs` — pure raw Skill invocation formatting and lexical dirname handling, exported only through `harness::skills`.
- `crates/pi-agent-core/src/harness/mod.rs` — public harness modules and value re-exports.
- `crates/pi-agent-core/tests/resources.rs` — focused field, option, mutation, ordering, duplicate, and `Debug` proof.
- `crates/pi-agent-core/tests/differential_truncate/resources.rs` — 12 plain-record differential fixtures (`13024..=13035`).
- `crates/pi-agent-core/tests/differential_truncate/prompt_templates.rs` — 15 prompt-helper differential fixtures (`13046..=13060`).
- `crates/pi-agent-core/tests/differential_truncate/skill_formatting.rs` — 8 Skill-formatting differential fixtures (`13061..=13068`).
- `oracle/truncate-oracle.ts` — persistent resource-record, prompt-template-helper, and Skill-formatting operations.
- `oracle/public-index-typecheck.mjs` — strict public-index type proof and pinned HEAD check.
- `run-tests.sh` — focused, differential, and oracle test-runner registration.

## Tests asserting this spec

- `crates/pi-agent-core/tests/resources.rs`
- `crates/pi-agent-core/tests/prompt_templates.rs`
- `crates/pi-agent-core/tests/skill_formatting.rs`
- `crates/pi-agent-core/tests/differential_truncate.rs`
- `crates/pi-agent-core/tests/differential_truncate/resources.rs`
- `crates/pi-agent-core/tests/differential_truncate/prompt_templates.rs`
- `oracle/public-index-typecheck.mjs`

## Known gaps (current cycle)

- Filesystem loading, YAML/frontmatter parsing, diagnostic production during loading, validation, lookup, invocation integration, model visibility, and harness integration remain deferred.

## Out of scope

- Rust generic `TSkill`/`TPromptTemplate` subtype parameters; the source generic capability is type-proven, while the approved Rust boundary uses concrete `Skill` and `PromptTemplate` values.
- Explicit JavaScript `undefined` versus absent-property identity; both map to Rust `None` because no pinned consumer observes own-property presence.
- Skill or prompt-template filesystem loading, YAML/frontmatter parsing, diagnostic collection/order during loading, validation, ignore/traversal rules, lookup, invocation integration, model visibility, and harness integration.
- Harness shallow copies, element aliasing, providers, tools, streams, serde, parsing, constructors, builders, validation, normalization, deduplication, fallbacks, and new dependencies.

## Loader IO failures

- Returned FileError records become the frozen diagnostics; native Node promise rejections propagate unchanged. They must not become diagnostics. [Proof](../porting/traceability/harness-nodejs-env.md#native-io-rejection-carrier).
