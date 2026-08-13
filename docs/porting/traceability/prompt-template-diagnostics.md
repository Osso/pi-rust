# Prompt-template diagnostic traceability

Pinned source: `packages/agent/src/harness/prompt-templates.ts` at commit `532f7d12cd19235081bd4d6e425bde185652d86f`.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `prompt-templates.ts:4` | `PromptTemplateDiagnosticCode` permits exactly four string literals | `harness::prompt_templates::PromptTemplateDiagnosticCode` has four closed variants and exact `as_str()` values; focused literal and equality proof |
| `prompt-templates.ts:6-16` | `PromptTemplateDiagnostic` is a mutable required record with `type`, `code`, `message`, and `path` fields in source order | `harness::prompt_templates::PromptTemplateDiagnostic` uses public mutable fields, `Debug` only, and exact Rust value types; focused construction, mutation, export, and string-boundary proof |
| `index.ts:77` | The package index wildcard-export includes the diagnostic declarations from the prompt-template module | Rust re-exports the three diagnostic types from `pi_agent_core::harness`; focused module/root imports prove both paths |

## Rust boundary

- `PromptTemplateDiagnosticCode` and `PromptTemplateDiagnosticType` derive only `Debug`, `Clone`, `Copy`, `PartialEq`, and `Eq`.
- `PromptTemplateDiagnostic` derives `Debug` only and remains publicly mutable.
- Rust `String` values preserve every valid Unicode scalar, NUL, CR/LF, punctuation, and XML-sensitive character without escaping, trimming, validation, or normalization.
- Invalid JavaScript runtime shapes and lone-surrogate strings remain outside the typed Rust boundary.

## Proof status

- Focused RED: `/tmp/pi-rust-prompt-template-diagnostic-red.out` captures unresolved imports only for the missing module/root diagnostic API; the proof ledger records exit 101.
- Focused GREEN: `/tmp/pi-rust-prompt-template-diagnostic-green.out` captures 4 passed and 0 failed; the proof ledger records exit 0, and production commit `25fe426` adds the value API, focused proof, and specification paths.
- Rust readability, formatting, and diff checks pass for the production-phase changes; the existing `parse_slice_match` length finding predates this slice and remains outside scope.
- Differential RED output `/tmp/pi-rust-prompt-template-diagnostic-differential-red.out` captures the existing non-JSON unsupported-operation failure at request `13121`; the proof ledger records exit 101.
- Differential GREEN output `/tmp/pi-rust-prompt-template-diagnostic-differential-green.out` captures the persistent differential test passing exactly eight request groups (`13121..=13128`); the proof ledger records exit 0.
- Oracle output `/tmp/pi-rust-prompt-template-diagnostic-oracle.out` captures strict typecheck and public-index proof completion; the proof ledger records exit 0.
- Readability output `/tmp/pi-rust-prompt-template-diagnostic-readability-summary.md` reports no new metric violations in the changed Rust fixture/runner or JavaScript public-index file; pre-existing TypeScript helper findings remain unchanged from the baseline.
- Final workspace/static gates, documentation closure, clean pins, and inherited verification remain pending.

## Exclusions

Prompt-template loading, sourced augmentation, `ExecutionEnv`, YAML/frontmatter parsing, filesystem behavior, diagnostic arrays/order, `toError`, dynamic errors, constructors, serde, validation, normalization, dependencies, and unrelated formatting helpers remain deferred.
