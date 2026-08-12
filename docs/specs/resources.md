# Agent-core skill and prompt resources

Agent-core exposes the dependency-free `Skill`, `PromptTemplate`, and concrete `AgentHarnessResources` value contracts from `packages/agent/src/harness/types.ts`; loading, formatting, and harness integration remain later slices. See the [resource traceability](../porting/traceability/resources.md) for source mappings.

## What it must do

### Skill

- [x] Expose mutable public `name`, `description`, `content`, `file_path`, and optional `disable_model_invocation` fields.
- [x] Preserve `None`, `Some(false)`, and `Some(true)` for `disable_model_invocation`.
- [x] Preserve empty, Unicode, supplementary, combining, embedded-NUL, and punctuation strings without normalization.

### Prompt template

- [x] Expose mutable public `name`, optional `description`, and `content` fields.
- [x] Preserve absent, empty, and non-empty descriptions.
- [x] Preserve empty, Unicode, supplementary, combining, embedded-NUL, and punctuation strings without normalization.

### Resource collection

- [x] Expose mutable public optional `prompt_templates` and `skills` arrays containing the concrete value types.
- [x] Preserve `None` versus `Some(vec![])`, array order, duplicate elements, nested mutation, and array replacement.
- [x] Export all three values from `pi_agent_core::harness` with `Debug` support and no additional traits or constructors.

## How it works

- [Resource traceability](../porting/traceability/resources.md)

## Implementation inventory

- `crates/pi-agent-core/src/harness/types.rs` — resource value definitions.
- `crates/pi-agent-core/src/harness/mod.rs` — public harness re-exports.
- `crates/pi-agent-core/tests/resources.rs` — focused field, option, mutation, ordering, duplicate, and `Debug` proof.
- `run-tests.sh` — focused test registration.

## Tests asserting this spec

- `crates/pi-agent-core/tests/resources.rs`

## Known gaps (current cycle)

- [ ] Add pinned public-index type proof and plain-record differential proof; no oracle changes are part of this focused production slice.
- [ ] Run final workspace gates, documentation audit, and independent verification.

## Out of scope

- Generic `TSkill`/`TPromptTemplate` subtype parameters; the approved Rust boundary uses concrete `Skill` and `PromptTemplate` values.
- Explicit JavaScript `undefined` versus absent-property identity; both map to Rust `None` because no pinned consumer observes own-property presence.
- Skill or prompt-template filesystem loading, YAML/frontmatter parsing, validation, diagnostics, ignore/traversal rules, formatting, XML escaping, lookup, invocation, and model visibility.
- Harness shallow copies, element aliasing, providers, tools, streams, serde, parsing, constructors, builders, validation, normalization, deduplication, fallbacks, and new dependencies.
