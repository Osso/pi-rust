# Agent-core skill and prompt resource traceability

Pinned source: `packages/agent/src/harness/types.ts` at commit `532f7d12cd19235081bd4d6e425bde185652d86f`; public type-only exports are `packages/agent/src/index.ts:109-136`.

This focused production slice ports only the dependency-free resource value contracts. Focused Rust proof is complete; pinned public-index type proof and plain-record differential proof remain pending and are intentionally excluded from this change.

| TypeScript source | Observable contract | Rust proof or gap |
|---|---|---|
| `types.ts:46-57` | `Skill` exposes mutable `name`, `description`, `content`, `filePath`, and optional `disableModelInvocation` fields | `Skill` exposes public `String` fields using `file_path` and `Option<bool>`; `resources.rs` covers construction, mutation, and optional states |
| `types.ts:59-67` | `PromptTemplate` exposes mutable `name`, optional `description`, and `content` fields | `PromptTemplate` exposes public `String`/`Option<String>` fields; focused tests cover absent, empty, non-empty, Unicode, NUL, and mutation cases |
| `types.ts:69-79` | `AgentHarnessResources<TSkill, TPromptTemplate>` exposes optional `promptTemplates` and `skills` arrays | Approved concrete `AgentHarnessResources` uses `Option<Vec<PromptTemplate>>` and `Option<Vec<Skill>>`; focused tests cover absent versus empty arrays, order, duplicates, nested mutation, and replacement |
| `index.ts:109-136` | `Skill`, `PromptTemplate`, and `AgentHarnessResources` are public type-only exports | Rust imports all three through `pi_agent_core::harness`; pinned public-index type proof is pending |
| `harness/system-prompt.ts:1-23` | Skill values are filtered and formatted from their public fields | Consumer mapping only; system-prompt formatting remains deferred |
| `harness/skills.ts:3,38-52,83-96` | Skill values are formatted, loaded, and optionally mapped to application subtypes | Consumer mapping only; filesystem loading and generic application subtypes remain deferred |
| `harness/prompt-templates.ts:2,30-33,70-86` | Prompt templates are loaded and optionally mapped to application subtypes | Consumer mapping only; filesystem loading and generic application subtypes remain deferred |
| `harness/agent-harness.ts:26,238,330-335,460-470` | Harness resources use `Skill`/`PromptTemplate`, and runtime methods shallow-copy arrays | Value fields are implemented; harness construction, shallow-copy, aliasing, and runtime methods remain deferred |
| `index.ts:109-136` | Type-only exports erase at runtime | No Rust runtime type-erasure claim; differential public-index proof remains pending |

## Approved Rust boundary

- `filePath` maps to `file_path`; `disableModelInvocation` maps to `disable_model_invocation`; `promptTemplates` maps to `prompt_templates`.
- Absent and explicit `undefined` optional properties map to `None`. `Some(false)` and `Some(vec![])` remain distinct from `None`.
- The public TypeScript resource interface permits custom subtype parameters. Alessio approved concrete Rust resources on August 12, 2026; no unbounded generics or invented traits are added.
- All three Rust structs derive only `Debug`. No constructors, validation, normalization, deduplication, serde, parser, builder, fallback, or dependency is added.
