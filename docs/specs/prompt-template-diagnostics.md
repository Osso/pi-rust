# Prompt-template diagnostics

This specification covers the prompt-template diagnostic value contract from `packages/agent/src/harness/prompt-templates.ts:4-16`. It defines the Rust value API; loading behavior remains deferred. The broader resource contract is documented in [resources](resources.md).

## What it must do

- [x] Expose `PromptTemplateDiagnosticCode` with exact literals `file_info_failed`, `list_failed`, `read_failed`, and `parse_failed`.
- [x] Expose `PromptTemplateDiagnosticType::Warning` with exact literal `warning`.
- [x] Expose a public mutable `PromptTemplateDiagnostic` with required fields `type`, `code`, `message`, and `path` in source order.
- [x] Preserve empty, ordinary, Unicode, supplementary, combining, NUL, CR/LF, punctuation, and XML-sensitive string values without normalization.
- [x] Expose the diagnostic values through `pi_agent_core::harness::prompt_templates` and `pi_agent_core::harness`.
- [x] Compare diagnostic value records directly with the pinned TypeScript source through exactly eight persistent differential request groups `13121..=13128` and strict public-index type proof.
- [x] Complete final workspace, static, readability, documentation, and clean-pin gates at code HEAD `4ab0686`: 234 workspace tests passed with 0 failures and 2 ignored; all-target check, all-feature Clippy with `-D warnings`, formatting, diff, and Node syntax exited 0; pinned source HEAD `532f7d12cd19235081bd4d6e425bde185652d86f` and clean worktrees were confirmed.

## How it works

- [Prompt-template diagnostic notes](../wiki/systems/prompt-template-diagnostics.md)
- [Prompt-template diagnostic traceability](../porting/traceability/prompt-template-diagnostics.md)

## Implementation inventory

- `crates/pi-agent-core/src/harness/prompt_templates.rs` — diagnostic code/type enums and value record.
- `crates/pi-agent-core/src/harness/mod.rs` — harness-root diagnostic re-exports.
- `crates/pi-agent-core/tests/prompt_template_diagnostic.rs` — focused literal, export, mutation, and string-boundary proof.
- `crates/pi-agent-core/tests/differential_truncate/prompt_template_diagnostic.rs` — eight pinned plain-record differential groups (`13121..=13128`).
- `oracle/truncate-oracle.ts` — direct pinned prompt-template diagnostic value adapter.
- `oracle/public-index-typecheck.mjs` — strict public-index interface and mutability proof.
- `run-tests.sh` — focused test registration.

## Tests asserting this spec

- `crates/pi-agent-core/tests/prompt_template_diagnostic.rs`
- `crates/pi-agent-core/tests/differential_truncate/prompt_template_diagnostic.rs`
- `oracle/public-index-typecheck.mjs`

## Known gaps (current cycle)

- [x] Add direct pinned TypeScript differential proof for the diagnostic values through request groups `13121..=13128` and strict public-index type proof.
- [x] Run final workspace, static, readability, documentation, and clean-pin gates after the complete slice: 234 workspace tests passed with 0 failures and 2 ignored; all-target check, all-feature Clippy with `-D warnings`, formatting, diff, and Node syntax exited 0.
- [ ] Complete inherited independent verification after the final proof and documentation closure.

## Out of scope

Prompt-template loading, sourced augmentation, `ExecutionEnv`, YAML/frontmatter parsing, filesystem behavior, diagnostic arrays/order, `toError`, dynamic errors, constructors, serde, validation, normalization, dependencies, and unrelated formatting helpers remain deferred.
