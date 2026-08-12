# Agent-core skill and prompt resource traceability

Pinned source: `packages/agent/src/harness/types.ts` at commit `532f7d12cd19235081bd4d6e425bde185652d86f`; public type-only exports are `packages/agent/src/index.ts:109-136`.

This bounded slice ports the dependency-free resource value contracts and pure prompt-template argument helpers. Focused Rust proof, pinned public-index type proof, and plain-record differential proof are complete for the value contracts; prompt-helper differential proof and loading/harness integration remain deferred.

| TypeScript source | Observable contract | Rust proof or gap |
|---|---|---|
| `types.ts:46-57` | `Skill` exposes mutable `name`, `description`, `content`, `filePath`, and optional `disableModelInvocation` fields | `Skill` exposes public `String` fields using `file_path` and `Option<bool>`; `resources.rs` covers construction, mutation, and optional states |
| `types.ts:59-67` | `PromptTemplate` exposes mutable `name`, optional `description`, and `content` fields | `PromptTemplate` exposes public `String`/`Option<String>` fields; focused tests cover absent, empty, non-empty, Unicode, NUL, and mutation cases |
| `types.ts:69-79` | `AgentHarnessResources<TSkill, TPromptTemplate>` exposes optional `promptTemplates` and `skills` arrays | Approved concrete `AgentHarnessResources` uses `Option<Vec<PromptTemplate>>` and `Option<Vec<Skill>>`; focused tests cover absent versus empty arrays, order, duplicates, nested mutation, and replacement |
| `index.ts:109-136` | `Skill`, `PromptTemplate`, and `AgentHarnessResources` are public type-only exports | `oracle/public-index-typecheck.mjs` imports all three from the absolute pinned public index; focused Rust imports compile through `pi_agent_core::harness` |
| `harness/system-prompt.ts:1-23` | Skill values are filtered and formatted from their public fields | Consumer mapping only; system-prompt formatting remains deferred |
| `harness/skills.ts:3,38-52,83-96` | Skill values are formatted, loaded, and optionally mapped to application subtypes | Consumer mapping only; filesystem loading and generic application subtypes remain deferred |
| `harness/prompt-templates.ts:2,30-33,70-86` | Prompt templates are loaded and optionally mapped to application subtypes | Consumer mapping only; filesystem loading and generic application subtypes remain deferred |
| `harness/prompt-templates.ts:217-239` | `parseCommandArgs` splits only on ASCII space/tab, removes matching quotes, preserves literal content, accepts unmatched quotes, and drops empty tokens | `harness::prompt_templates::parse_command_args`, focused parser tests in `tests/prompt_templates.rs`; direct differential proof remains pending |
| `harness/prompt-templates.ts:244-258` | `substituteArgs` applies positional, slice, aggregate, and `$@` replacement passes in exact order, including bounded indices and JavaScript replacement-string semantics | `harness::prompt_templates::substitute_args`, focused substitution tests in `tests/prompt_templates.rs`; direct differential proof remains pending |
| `harness/prompt-templates.ts:260-262` | `formatPromptTemplateInvocation` substitutes only `template.content` and defaults omitted arguments to an empty list | `harness::prompt_templates::format_prompt_template_invocation`; callers pass `&[]` for the source default; focused wrapper tests; direct differential proof remains pending |
| `harness/agent-harness.ts:26,238,330-335,460-470` | Harness resources use `Skill`/`PromptTemplate`, and runtime methods shallow-copy arrays | Value fields are implemented; harness construction, shallow-copy, aliasing, and runtime methods remain deferred |
| `index.ts:109-136` | Type-only exports erase at runtime | The generated public-index fixture proves compile-time exports; runtime proof uses plain records and makes no constructor or type-erasure claim |

## Approved Rust boundary

- `filePath` maps to `file_path`; `disableModelInvocation` maps to `disable_model_invocation`; `promptTemplates` maps to `prompt_templates`.
- Absent and explicit `undefined` optional properties map to `None`. `Some(false)` and `Some(vec![])` remain distinct from `None`.
- The public TypeScript resource interface permits custom subtype parameters; the strict fixture proves that capability with exact custom extensions. Alessio approved concrete Rust resources on August 12, 2026; Rust intentionally omits those generic parameters, with no unbounded generics or invented traits.
- All three Rust structs derive only `Debug`. No constructors, validation, normalization, deduplication, serde, parser, builder, fallback, or dependency is added to the value types. Prompt helpers use only the Rust standard library and are exported at `pi_agent_core::harness::prompt_templates`; they are not re-exported at package level because the pinned `index.ts` omits them.

## Pinned proof

- `oracle/public-index-typecheck.mjs` validates every source-lock field, verifies the exact pinned TypeScript HEAD, and runs a generated strict fixture imported from the public index. The fixture proves exact metadata/resource structures, mutable fields and arrays, explicit `undefined` acceptance, and exact generic subtype preservation with custom skill/template extensions.
- `crates/pi-agent-core/tests/differential_truncate/resources.rs` sends exactly 12 `resource_values` requests (`13024..=13035`) through the persistent utility oracle. The fixtures cover absent and explicit-`undefined` optional fields, `false`/`true`, empty strings/arrays, Unicode/supplementary/combining/NUL/punctuation strings, ordered duplicate arrays, and both resource collections.
- `crates/pi-agent-core/tests/prompt_templates.rs` covers the pure helper contract with eight focused behavioral tests. Differential requests are reserved for the next proof slice after `13045`.
- The runtime proof constructs plain mutable JavaScript records only; it does not claim runtime constructors, source type erasure behavior beyond normalized values, or harness shallow-copy/aliasing semantics.
