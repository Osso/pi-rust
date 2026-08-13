# TypeScript compatibility

The Rust port uses the pinned Pi TypeScript worktree as a behavioral oracle. Implementation structure may differ, but every representable observable result must match unless Alessio approves a documented deviation.

## What it must do

- [x] Refuse differential proof when the source worktree is not at the pinned commit.
- [x] Execute the real pinned TypeScript functions through the worktree-local `tsx` runtime.
- [x] Send identical fixtures to TypeScript and Rust and compare normalized values and errors.
- [x] Execute ordered `push` and `end` steps against one TypeScript and Rust decoder, comparing every success and error without replacing the instance.
- [x] Map each ported TypeScript statement and branch to behavioral proof or an explicit gap.
- [x] Represent protocol numbers with schema-specific Rust types instead of preserving JavaScript or CBOR numeric categories.
- [ ] Classify discrepancies as a Rust bug, TypeScript bug, approved deviation, or underspecified behavior before changing semantics.
- [x] Stop and ask Alessio when Rust cannot reproduce TypeScript behavior within the approved typed-core boundary.
- [x] Classify the RPC JSON serializer as semantic JSON rather than byte-identical `JSON.stringify`, approved August 12, 2026.
- [x] Classify truncation limits as typed non-negative integers and single-line truncation as Unicode-scalar-safe, approved August 12, 2026.
- [x] Preserve the shell sanitizer's explicit code-point ranges rather than applying broad Unicode control categories.
- [x] Preserve exact filesystem/execution error-code literals, names, messages, constructor ordering, optional path, and typed causes over the Rust-representable boundary.
- [x] Map typed-error constructors, public fields, mutation, and error-chain behavior to focused Rust tests; compare normalized constructor and cause values through 14 pinned TypeScript differential fixtures (`13000..=13013`) using the public agent package exports.
- [x] Preserve distinct compaction and branch-summary code literals, names, messages, constructor ordering, public mutation, and typed causes in focused Rust tests.
- [x] Compare 10 normalized compaction and branch-summary constructor and cause values through direct pinned TypeScript differential fixtures (`13036..=13045`) using the public agent package exports; the persistent harness-error proof contains 24 typed-error fixtures total.
- [x] Preserve exact `FileKind` literals and the mutable `FileInfo` field contract with direct `f64` storage and focused bit-preservation proof.
- [x] Compare erased `FileInfo`/`FileKind` TypeScript types through a strict generated public-index fixture and compare plain-record binary64 values through 10 pinned differential fixtures (`13014..=13023`), without claiming a source runtime constructor.
- [x] Preserve focused `Skill`, `PromptTemplate`, and concrete `AgentHarnessResources` fields, optional states, public mutation, order, duplicates, and `Debug` behavior.
- [x] Compare resource erased types through the pinned public index and compare plain records through 12 direct differential fixtures (`13024..=13035`), while documenting the approved omission of Rust generic subtype parameters.
- [x] Compare prompt-template argument parsing, substitution, and content formatting through 15 direct pinned differential fixtures (`13046..=13060`) using the persistent utility oracle and direct `prompt-templates.ts` module import.
- [x] Preserve focused image MIME detection and standard Base64 behavior through `crates/pi-agent-core/tests/image.rs`.
- [x] Compare image MIME detection and Base64 output directly with the pinned TypeScript module through 12 persistent differential request groups (`13069..=13080`) in `crates/pi-agent-core/tests/differential_truncate/image.rs`.
- [x] Preserve the focused edit-diff line-ending and BOM helper contract in `crates/pi-agent-core/tests/edit_line_endings.rs`.
- [x] Compare the edit-diff line-ending and BOM helpers directly with the pinned TypeScript source through eight persistent request groups (`13081..=13088`) in `crates/pi-agent-core/tests/differential_truncate/edit_line_endings.rs`.
- [x] Preserve exact compaction file-operation section formatting through `crates/pi-agent-core/tests/file_operations.rs`.
- [x] Compare compaction file-operation formatting directly with the pinned TypeScript source through seven persistent request groups (`13089..=13095`) in `crates/pi-agent-core/tests/differential_truncate/file_operations.rs`.
- [x] Preserve exact public summary marker constants through `crates/pi-agent-core/tests/summary_markers.rs`.
- [x] Compare summary marker constants and package-index exports directly with the pinned TypeScript source through request `13096` and strict public-index type proof.
- [x] Preserve the focused value-only `BashExecutionMessage` role, mutable fields, optionality, unrestricted binary64 values, and module/root exports from `messages.ts:19-29`.
- [x] Compare the value-only `BashExecutionMessage` directly with the pinned TypeScript interface through eight differential request groups (`13097..=13104`) and strict public-index type proof.
- [x] Preserve the focused value-only `BranchSummaryMessage` role, mutable fields, unrestricted timestamp binary64 values, and module/root exports from `messages.ts:40-45`.
- [x] Compare the value-only `BranchSummaryMessage` directly with the pinned TypeScript interface through eight differential request groups (`13105..=13112`) and strict public-index type proof.
- [x] Preserve mutable `CompactionSettings`, strict `shouldCompact` threshold behavior, unrestricted binary64 arithmetic, and module/root exports from `compaction.ts:152-159,251-254`; compare directly through eight differential request groups (`13113..=13120`) and strict public-index type proof.
- [x] Preserve module-local `SUMMARIZATION_SYSTEM_PROMPT` from `compaction.ts:489-491` as an exact 310-byte ASCII Rust const with no harness-root/package export; compare directly through request `13129` and a standalone direct-module type proof, without claiming public-index proof.
- [x] Preserve module-local mutable `CutPointResult` fields and unrestricted binary64 values from `compaction.ts:450-457`; the package index exports `findCutPoint` but omits `type CutPointResult`, so Rust remains without a harness-root/package export; compare final source-shaped plain records through eight direct differential groups (`13130..=13137`) and prove required mutable fields through standalone direct-module type proof.
- [x] Preserve module-local mutable `ContextUsageEstimate` fields and unrestricted binary64 values from `compaction.ts:200-208`; the package index exports `estimateContextTokens` but omits `type ContextUsageEstimate`, so Rust remains without a harness-root/package export; focused, eight-group differential, direct-module shape/mutation, workspace, static, readability, documentation, and clean-pin proof pass, without producer/runtime-construction or public-index claims.
- [x] Preserve module-local mutable `CompactionDetails` fields from `compaction.ts:30-35`; the package index omits the interface, so Rust remains module-only; compare final source-shaped records through eight direct differential groups (`13156..=13163`) and prove exact direct-module shape/mutation without extraction or orchestration claims.
- [x] Preserve the closed `QueueMode` value from `types.ts:44-50`, exact `all`/`one-at-a-time` literals, and harness-root export through focused behavioral tests; compare both final source-shaped literals through exactly one persistent differential request group (`13164`) and prove the exact package-index union with strict valid/invalid assignments. Runtime constructors, parsing, validation, and queue behavior remain excluded.
- [x] Preserve the closed `ToolExecutionMode` value from `types.ts:34-42`, exact `sequential`/`parallel` literals, and harness-root export through focused behavioral tests; compare both final source-shaped literals through exactly one persistent differential request group (`13165`) and prove the exact package-index union with strict valid/invalid assignments. Unsuppressed targeted Clippy RED/GREEN, workspace, static, readability, documentation, clean-pin, and inherited verification pass. Runtime constructors, parsing, validation, invalid runtime inputs, scheduling, execution, and event ordering remain excluded.
- [ ] `DEFAULT_COMPACTION_SETTINGS` remains deferred: no approved safe Rust API representation currently preserves both direct package-level object access and shared mutable identity. A Rust `const` value is copied at each use, a factory changes export shape, and `static mut` requires unsafe access. The pinned probe reports `reserveTokens: 0`, `keepRecentTokens: null`, and `sameObject: true`; `null` is invalid at the approved typed Rust boundary and intentionally excluded. Record the discrepancy; do not add a fallback.
- [x] Preserve focused `PromptTemplateDiagnosticCode`, `PromptTemplateDiagnosticType`, and mutable `PromptTemplateDiagnostic` values and module/root exports from `prompt-templates.ts:4-16`.
- [x] Compare prompt-template diagnostic values directly with the pinned TypeScript source through exactly eight persistent differential request groups (`13121..=13128`) and strict public-index type proof.
- [x] Preserve exact coding-agent auth command names, usage strings, and help selection over Rust-representable strings.
- [x] Preserve exact coding-agent auth parser errors, argument ordering, flags, and field-specific expiry-number behavior against the pinned source.
- [x] Preserve pure config-value command classification and environment-reference parsing against the pinned source.
- [x] Preserve config-value missing/configured status and environment truthiness against the pinned source.
- [x] Preserve `ParsedSkillBlock` fields and `parse_skill_block` behavior from `packages/coding-agent/src/core/skill-block.ts:1-22`, including anchored LF grammar, candidate-closing-delimiter backtracking, JavaScript trim semantics, and exclusion of lone UTF-16 surrogates at the Rust scalar-string boundary; the pinned package export is near `packages/coding-agent/src/index.ts:281`.
- [x] Compare the skill-block parser through exactly 10 persistent differential requests (`13138..=13147`), with runtime public-index proof separated from direct-module exact-shape/mutability typeproof; current focused, differential, and oracle evidence is recorded in the [dedicated spec](skill-block.md) and [traceability](../porting/traceability/skill-block.md).

## How it works

- [Differential port system](../wiki/systems/differential-port.md)

## Implementation inventory

- `docs/porting/source-lock.json` — pins the read-only TypeScript oracle.
- `oracle/` — invokes selected TypeScript source functions.
- `crates/*/tests/differential_*.rs` — compares Rust results with the oracle.
- `docs/porting/traceability/` — maps pinned source statements to proof.

## Tests asserting this spec

- `crates/pi-protocol/tests/differential_protocol.rs`
- `crates/pi-protocol/tests/model_cost.rs`
- `crates/pi-protocol/tests/model_metadata.rs`
- `crates/pi-protocol/tests/schema_leaves.rs`
- `crates/pi-protocol/tests/usage.rs`
- `crates/pi-protocol/tests/content_leaves.rs`
- `crates/pi-protocol/tests/session_metadata.rs`
- `crates/pi-protocol/tests/protocol_error_code.rs`
- `crates/pi-protocol/tests/commands.rs`
- `crates/pi-protocol/tests/client_hello.rs`
- `crates/pi-protocol/tests/message_envelopes.rs`
- `crates/pi-protocol/tests/differential_protocol/commands.rs`
- `crates/pi-protocol/tests/differential_protocol/client_hello.rs`
- `crates/pi-protocol/tests/differential_protocol/message_envelopes.rs`
- `crates/pi-protocol/tests/differential_protocol/content_leaves.rs`
- `crates/pi-protocol/tests/differential_protocol/model_metadata.rs`
- `crates/pi-protocol/tests/differential_protocol/protocol_error_code.rs`
- `crates/pi-protocol/tests/differential_protocol/schema_leaves.rs`
- `crates/pi-protocol/tests/differential_protocol/session_metadata.rs`
- `crates/pi-protocol/tests/differential_protocol/usage.rs`
- `crates/pi-coding-agent/tests/rpc_jsonl.rs`
- `crates/pi-coding-agent/tests/differential_rpc_jsonl.rs`
- `crates/pi-coding-agent/tests/auth_command.rs`
- `crates/pi-coding-agent/tests/differential_auth_command.rs`
- `crates/pi-coding-agent/tests/config_value_references.rs`
- `crates/pi-coding-agent/tests/config_value_status.rs`
- `crates/pi-coding-agent/tests/differential_config_value_references.rs`
- `crates/pi-coding-agent/tests/skill_block.rs`
- `crates/pi-coding-agent/tests/differential_skill_block.rs`
- `oracle/skill-block-typecheck.mjs`
- `crates/pi-agent-core/tests/truncate.rs`
- `crates/pi-agent-core/tests/differential_truncate.rs`
- `crates/pi-agent-core/tests/harness_errors.rs`
- `crates/pi-agent-core/tests/compaction_errors.rs`
- `crates/pi-agent-core/tests/context_usage_estimate.rs`
- `crates/pi-agent-core/tests/differential_truncate/errors.rs`
- `crates/pi-agent-core/tests/differential_truncate/context_usage_estimate.rs`
- `crates/pi-agent-core/tests/queue_mode.rs`
- `crates/pi-agent-core/tests/differential_truncate/queue_mode.rs`
- `crates/pi-agent-core/tests/tool_execution_mode.rs`
- `crates/pi-agent-core/tests/differential_truncate/tool_execution_mode.rs`
- `crates/pi-agent-core/tests/file_info.rs`
- `crates/pi-agent-core/tests/resources.rs`
- `crates/pi-agent-core/tests/sanitize_binary_output.rs`
- `crates/pi-agent-core/tests/differential_truncate/sanitize.rs`
- `crates/pi-agent-core/tests/differential_truncate/file_info.rs`
- `crates/pi-agent-core/tests/differential_truncate/resources.rs`
- `crates/pi-agent-core/tests/differential_truncate/prompt_templates.rs`
- `crates/pi-agent-core/tests/prompt_template_diagnostic.rs`
- `crates/pi-agent-core/tests/differential_truncate/prompt_template_diagnostic.rs`
- `crates/pi-agent-core/tests/image.rs`
- `crates/pi-agent-core/tests/differential_truncate/image.rs`
- `crates/pi-agent-core/tests/edit_line_endings.rs`
- `crates/pi-agent-core/tests/differential_truncate/edit_line_endings.rs`
- `oracle/public-index-typecheck.mjs`
- `oracle/context-usage-estimate-typecheck.mjs`

## Known gaps (current cycle)

- [ ] Classify and resolve the payload-above-4-GiB proof gap.

## Out of scope

- Invalid JavaScript runtime input types at typed Rust API boundaries, approved August 12, 2026.
- JavaScript lone-surrogate strings that Rust `String` cannot represent.
- Arbitrary JavaScript runtime values passed to generic error normalization or causes.
- JavaScript error property descriptors, own-property ordering, JSON serialization, stack contents, and prototype/`instanceof` identity.
- Any cause representation beyond the approved `Option<Box<dyn std::error::Error + 'static>>` typed Rust boundary.
- Compaction and branch-summary orchestration, model calls, retries, token budgets, abort execution, provider integration, and session mutation.
- Skill filesystem loading, prompt-template filesystem loading/YAML parsing, validation, lookup, invocation integration, model visibility, and harness resource-copy behavior; completed pure Skill and prompt-template formatting remain separate bounded slices.
- Image decoding, file I/O, data URLs, compression, provider integration, and protocol `ImageContent` behavior outside the typed schema contract.
- Fuzzy matching, edit application, diff generation, file I/O, and edit-tool orchestration beyond the focused line-ending/BOM helpers.
- Rust generic resource subtype parameters; the pinned public-index proof demonstrates the TypeScript capability, while the approved Rust boundary uses concrete `Skill`, `PromptTemplate`, and `AgentHarnessResources` values.
- Positive integer-valued JavaScript schema values above `u64::MAX` for typed integer fields, approved August 12, 2026.
- Malformed or manually forged oracle requests; the Rust harness emits only typed operations and valid hexadecimal bytes.
- Exact JavaScript allocation exception identity. Approved Rust behavior returns `FrameDecodeError::Allocation` and terminally fails the decoder.
- Generic public CBOR or JavaScript-number value models; typed schema fields use their actual Rust numeric types.
- Refactoring behavior while translating it.
- Treating TypeScript as the intended-behavior authority when a specification or invariant contradicts it.
- Byte-identical `JSON.stringify` output in RPC JSONL. Stock `serde_json` semantic output was approved August 12, 2026; exact number-spelling discrepancies are pinned against the source, Rust map order follows the selected serializer, and NaN/infinities are proven to match as `null`.
- Negative, fractional, NaN, and infinite JavaScript truncation limits; Rust uses non-negative integer counts.
- UTF-16 code-unit splitting and lone-surrogate output from JavaScript `truncateLine`; Rust truncates at Unicode scalar boundaries.
- Claiming full Pi compatibility before all package contracts are ported and integrated.
