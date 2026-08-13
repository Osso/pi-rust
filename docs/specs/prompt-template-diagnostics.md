# Prompt-template diagnostics

This specification covers the prompt-template diagnostic value contract from `packages/agent/src/harness/prompt-templates.ts:4-16`. It defines the Rust value API; loading behavior remains deferred.

## What it must do

- [x] Expose `PromptTemplateDiagnosticCode` with exact literals `file_info_failed`, `list_failed`, `read_failed`, and `parse_failed`.
- [x] Expose `PromptTemplateDiagnosticType::Warning` with exact literal `warning`.
- [x] Expose a public mutable `PromptTemplateDiagnostic` with required fields `type`, `code`, `message`, and `path` in source order.
- [x] Preserve empty, ordinary, Unicode, supplementary, combining, NUL, CR/LF, punctuation, and XML-sensitive string values without normalization.
- [x] Expose the diagnostic values through `pi_agent_core::harness::prompt_templates` and `pi_agent_core::harness`.
- [x] Compare diagnostic value records directly with the pinned TypeScript source through exactly eight persistent differential request groups `13121..=13128` and strict public-index type proof.
- [ ] Complete final workspace, readability, documentation, and clean-pin gates.

## How it works

- [Prompt-template diagnostic notes](../wiki/systems/prompt-template-diagnostics.md)
- [Prompt-template diagnostic traceability](../porting/traceability/prompt-template-diagnostics.md)

## Implementation inventory

- `crates/pi-agent-core/src/harness/prompt_templates.rs` — diagnostic code/type enums and value record.
- `crates/pi-agent-core/src/harness/mod.rs` — harness-root diagnostic re-exports.
- `crates/pi-agent-core/tests/prompt_template_diagnostic.rs` — focused literal, export, mutation, and string-boundary proof.
- `run-tests.sh` — focused test registration.

## Tests asserting this spec

- `crates/pi-agent-core/tests/prompt_template_diagnostic.rs`

## Known gaps (current cycle)

- [x] Add direct pinned TypeScript differential proof for the diagnostic values through request groups `13121..=13128` and strict public-index type proof.
- [ ] Run final workspace, static, readability, documentation, and clean-pin gates after the complete slice.

## Out of scope

Prompt-template loading, sourced augmentation, `ExecutionEnv`, YAML/frontmatter parsing, filesystem behavior, diagnostic arrays/order, `toError`, dynamic errors, constructors, serde, validation, normalization, dependencies, and unrelated formatting helpers remain deferred.
