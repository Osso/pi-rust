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
- [x] Deliver immutable push-time stream snapshots rather than TS delayed-consumer mutable aliases, approved by user October 20, 2026 (“Approve snapshot deviation”). Existing differentials prove normalized TS push-time parity; scratch fields are absent on both sides at delayed consumption. Evidence and event counts: [OpenAI Responses shared](../porting/traceability/openai-responses-shared.md#discrepancies), [Mistral conversations](../porting/traceability/mistral-conversations.md#approved-delayed-consumer-snapshot-deviation-user-2026-10-20).
- [x] Session event listeners cannot abort session operations. At pin `532f7d12`, a throwing TS listener escaped `AgentSession._emit` into `abort()`, so runtime replacement stopped before disposing the outgoing session. User October 20, 2026 (“fix the problem at the root” in TS Pi): TS Pi `94a7bbecf` isolates listener failures (session, agent-core `processEvents`, extension `emitError`, multi-agent store, resident console transport) and reports them via `console.error`. Rust listeners are infallible callbacks, matching the fixed behavior; this is a post-pin TS delta to carry into the later re-pin. SDK example 13 reports writer failure after the transition ([traceability](../porting/traceability/sdk-examples.md)).
- [x] Modal-editor normal-mode mappings use own keys only (post-pin TS fix `3abfead27`, user-approved October 20, 2026). Escape followed by chunks `constructor` and `toString` inserts `constructortoString` and keeps NORMAL mode. Native regression runs without the pinned oracle, which throws for inherited names; carry this delta into the later re-pin ([traceability](../porting/traceability/extension-examples-editors.md)).
- [x] Classify truncation limits as typed non-negative integers and single-line truncation as Unicode-scalar-safe, approved August 12, 2026.
- [x] Preserve the shell sanitizer's explicit code-point ranges rather than applying broad Unicode control categories.
- [x] Preserve exact filesystem/execution error-code literals, names, messages, constructor ordering, optional path, and typed causes over the Rust-representable boundary.
- [x] Keep Node filesystem formatting inside local read/write/edit/ls operations; preserve custom-operation messages except where pinned tools explicitly transform them. Prove remote read/write/edit/bash through the opt-in SSH example, including real TUI error output and U+FFFD projection; [proof and interface limits](../porting/traceability/core-ssh-errors.md).
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
- [x] Preserve the agent-harness `ThinkingLevel` value from `types.ts:293-298`, exact eight literals including `ultra`, and harness-root export through focused behavioral tests. Compare one source-shaped plain-value differential group (`13166`) after ToolExecutionMode (`13165`), and prove the exact agent-package public-index union with all eight valid assignments plus rejected empty/`extreme` literals. Focused/differential Clippy, workspace, static, readability, documentation, dependency, clean-pin, and inherited verification pass. The distinct protocol schema omits `ultra`; this slice does not alter or depend on `pi_protocol::ThinkingLevel`. Parsing, validation, provider/model behavior, defaults, and protocol integration remain excluded.
- [x] Preserve `AgentToolExecutionContext` from `types.ts:387-393` as a public mutable `started_at: f64`/`agent_id: Option<String>` record with `Debug` only and harness-root export. Focused production GREEN is 5/0; exactly eight source-shaped differential groups (`13168..=13175`) and strict public-index interface proof pass. Workspace 271/0/2, static, oracle/Node, formatting/diff, readability, documentation, dependency/lock, clean-pin, and inherited verification gates pass. Producers, lifecycle/timing validation, tool callbacks/results, external content, provider usage, orchestration, sessions, providers, models, constructors, parser/serde, defaults, fallbacks, invalid runtime inputs, protocol behavior, coding-agent behavior, and dependencies remain excluded.
- [x] Preserve package-exported `BeforeToolCallResult` from `packages/agent/src/types.ts:55-69` (`61-69` declaration; `index.ts:145`) as a `Debug`-only public mutable `block: Option<bool>`, `reason: Option<String>`, `terminate: Option<bool>` record through `pi_agent_core::harness::types` and the harness root. Exactly one source-shaped differential request (`13180`) with eleven independent state-tag fixtures, structured RED/GREEN, and real-index optional/mutable/undefined/excess-property proof pass. Workspace `286/0/2`, check, all-feature Clippy, aggregate oracle/typeproof, Node syntax, formatting/diff, readability, documentation, dependency/lock, clean-pin, and inherited verification gates pass through proof-readability HEAD `2bb4d86`. Blocking/error/default-message behavior, batch termination, callbacks, tool execution, orchestration, JavaScript object identity, arbitrary runtime values, and omission-versus-`undefined` identity remain excluded.
- [x] Preserve direct-module `Edit` and `AppliedEditsResult` from `packages/agent/src/harness/tools/edit-diff.ts:187-195` as `Debug`-only public mutable `old_text: String`/`new_text: String` and `base_content: String`/`new_content: String` records through `pi_agent_core::harness::tools::edit_diff`, with no tools/harness/crate-root re-export. Requests `13181` and `13183` compare four independent raw records each; the shared direct-module proof verifies required mutable strings and four exact unsuppressed invalid diagnostics for each record. Production/proof commits are `7085ae5`/`4b5fa7b` and `39bb794`/`27ac88c`. Workspace `296/0/2`, check, all-feature Clippy, aggregate oracle/typeproof, Node syntax, formatting/diff, readability, dependency/lock, and clean-pin gates pass at `b27569b`; inherited verification passes at docs-only HEAD `76d759f` (`OVERALL: PASS`). Edit application, fuzzy matching, normalization, matching/validation, offsets, ordering, filesystem/tool behavior, and extra traits remain excluded.
- [x] Preserve direct-module `FuzzyMatchResult` from `packages/agent/src/harness/tools/edit-diff.ts:171-184` as a `Debug`-only public mutable `found: bool`, `index: f64`, `match_length: f64`, `used_fuzzy_match: bool`, and `content_for_replacement: String` record through `pi_agent_core::harness::tools::edit_diff`, with no tools/harness/crate-root re-export. Request `13184` compares eight independent binary64/source-shaped cases; real direct-module proof passes two valid assignments and nine exact invalid diagnostics. Production/proof commits are `1a8ba0c`/`62e5770`; focused RED/GREEN, targeted Clippy/typecheck/fmt/diff/readability, workspace `300/0/2` across 66 result lines, workspace check, all-feature Clippy, aggregate oracle/Node, docs/scope/dependency/lock, and clean-pin audit pass at `f802bf1`. Inherited verification passes at docs-only closure HEAD `0fdd0ed` (`OVERALL: PASS`). `f64` preserves unrestricted binary64 patterns; fuzzyFindText, normalization, matching/index semantics and invariants, edit application, filesystem/tool behavior, and producer/runtime-interface claims remain excluded.
- [x] Preserve package-visible `SourceInfo` and `createSyntheticSourceInfo` from coding-agent `core/source-info.ts` through `pi_coding_agent::core::source_info` and crate-root `SourceInfo`/constructor re-exports. Production `37a8cc2`, request/proof `d3a4b08`, and approved public `SyntheticSourceInfoOptions` carrier preserve scope/origin defaults, optional baseDir, fresh output, and options nonmutation. Workspace `293/0/2`, check, all-feature Clippy, aggregate oracle/typeproof, Node syntax, formatting/diff, readability, dependency/lock, and clean-pin gates pass at `e966d3d`; inherited verification passes at docs-only HEAD `e48a6c0`. `PathMetadata`/`createSourceInfo`, package-manager, extensions, resource, and session behavior remain explicit gaps.
- [x] Preserve package-exported coding-agent `formatSize` from `core/tools/truncate.ts:61-69` through `pi_coding_agent::core::tools::truncate::format_size`, the tools-index equivalent, and crate-root `pi_coding_agent::format_size`. Production/proof commits are `0a46038`/`38fdc97`; focused GREEN is 3/0 across 16 hard-coded cases, request `13185` differential GREEN is 1/0 across 16 independent cases, and real direct/tools/package proof passes exact `(number) => string`, 3 valid assignments, and 3 wrong-string diagnostics. The wrapper delegates to identical approved agent-core `usize` behavior; workspace `304/0/2` across 68 result lines, workspace check, all-target/all-feature Clippy with `-D warnings`, aggregate oracle/typeproof/Node, fmt/diff, Rust/TS readability, exact wrapper/export/scope/dependency/docs/lock/clean-pin audit pass at executable/docs HEAD `a8a376e`. Evidence is `/tmp/pi-rust-coding-format-size-final-*.out` plus the final audit; inherited verification passes at docs-only closure HEAD `52ff0b0` (`OVERALL: PASS`). Other coding-agent truncation types/functions remain excluded.
- [x] Preserve coding-agent truncation constants from `core/tools/truncate.ts:11-13` through direct `pi_coding_agent::core::tools::truncate` paths, with only `DEFAULT_MAX_LINES`/`DEFAULT_MAX_BYTES` re-exported by tools/package indexes and `GREP_MAX_LINE_LENGTH` direct-module-only. Request `13186` differential GREEN is 1/0 from one independent source-shaped request; the dedicated real-source proof passes exact const declarations/types (`2000` literal, broad `number`, `500` literal), two-index aliases, valid assignments, five exact invalid diagnostics, direct/tools-index runtime values/presence, and compiler-proven package-root alias identity. `usize` remains the approved typed non-negative limit boundary; truncation types/functions and algorithms remain excluded.
- [x] Preserve direct-module `isValidThinkingLevel(level: string): level is ThinkingLevel` from `packages/coding-agent/src/cli/args.ts:67-70` as `pi_coding_agent::cli::args::is_valid_thinking_level(&str) -> bool` over the exact eight agent-harness literals; the package index exports only `Args`/`parseArgs`, so no package or crate-root validator export is claimed. Focused production is 2/0; request `13177` compares all valid literals plus representative invalid Rust strings against the actual source function. Standalone proof checks the exact type predicate, caller narrowing, runtime cases, and private-table non-export. Workspace 276/0/2, static, aggregate oracle/typeproof, Node syntax, formatting/diff, readability, documentation, dependency/lock, clean-pin, and inherited verification gates pass. The TypeScript caller-narrowing predicate is documented as a language-boundary discrepancy; no parser/`Option` substitute is added. `Mode`, `Args`, `parseArgs`, diagnostics, help, dispatch, public table export, parser/enum conversion, provider/model/default behavior, protocol, and unrelated APIs remain excluded.
- [x] Preserve package-exported `MAX_PERSISTED_TOOL_RESULT_CONTENT_BYTES` and `TOOL_RESULT_TRUNCATION_MARKER` from `packages/coding-agent/src/core/session-tool-output.ts:19-20` through `pi_coding_agent::core::session_tool_output` and crate-root exports; the `usize` byte-limit mapping is a narrow exact-literal exception and does not generalize unrestricted JavaScript `number` fields. Focused production is 1/0.
- [x] Compare both tool-output persistence constants through exactly one differential request (`13178`) and the strict coding-agent public-index declaration proof. Differential GREEN is 1/0 and the runtime package-index probe passes; symbol-scoped compiler proof uses the real pinned index/defining module, exact literal types, and real-index aliases. Workspace 278/0/2, static, aggregate oracle/typeproof, dedicated typeproof, Node syntax, formatting/diff, readability, documentation, dependency/lock, clean-pin, and inherited verification gates pass. Migration, truncation, serialization, filesystem/session/message/content, and malformed-recovery behavior remain excluded.
- [ ] `DEFAULT_COMPACTION_SETTINGS` remains deferred: no approved safe Rust API representation currently preserves both direct package-level object access and shared mutable identity. A Rust `const` value is copied at each use, a factory changes export shape, and `static mut` requires unsafe access. The pinned probe reports `reserveTokens: 0`, `keepRecentTokens: null`, and `sameObject: true`; `null` is invalid at the approved typed Rust boundary and intentionally excluded. Record the discrepancy; do not add a fallback.
- [x] Preserve focused `PromptTemplateDiagnosticCode`, `PromptTemplateDiagnosticType`, and mutable `PromptTemplateDiagnostic` values and module/root exports from `prompt-templates.ts:4-16`.
- [x] Compare prompt-template diagnostic values directly with the pinned TypeScript source through exactly eight persistent differential request groups (`13121..=13128`) and strict public-index type proof.
- [x] Preserve exact coding-agent auth command names, usage strings, and help selection over Rust-representable strings.
- [x] Preserve exact coding-agent auth parser errors, argument ordering, flags, and field-specific expiry-number behavior against the pinned source.
- [x] Preserve pure config-value command classification and environment-reference parsing against the pinned source.
- [x] Preserve config-value missing/configured status and environment truthiness against the pinned source.
- [x] Preserve direct-module `Mode = "text" | "json" | "rpc"` from `packages/coding-agent/src/cli/args.ts:13`; the pinned coding-agent package index exports only `Args`/`parseArgs`, so Rust exposes only `pi_coding_agent::cli::args::Mode` with no `cli`/crate-root re-export. Parsing, help, dispatch, and runtime consumers remain excluded.
- [x] Compare direct-module `Mode` through exactly one differential request `13176` containing `text`, `json`, and `rpc` in source order, plus a standalone direct-module typeproof rejecting `""`, `"tui"`, and `"RPC"`; no package-index or runtime type-export claim is made. Workspace 273/0/2, static, aggregate oracle/typeproof, Node syntax, formatting/diff, readability, documentation, dependency/lock, clean-pin, and inherited verification gates pass.
- [x] Preserve direct-module `DEFAULT_THINKING_LEVEL: ThinkingLevel = "medium"` from `packages/coding-agent/src/core/defaults.ts:1-3` through the exact local agent-core dependency; the pinned coding-agent package index does not export this constant, so Rust adds no core/crate-root re-export. Resolver, session, SDK, CLI, provider, and protocol behavior remain excluded.
- [x] Compare the actual direct-module default through persistent differential request `13167` and a standalone direct-module type/value proof; no coding-agent package-index claim is made.
- [x] Preserve `ParsedSkillBlock` fields and `parse_skill_block` behavior from `packages/coding-agent/src/core/skill-block.ts:1-22`, including anchored LF grammar, candidate-closing-delimiter backtracking, JavaScript trim semantics, and exclusion of lone UTF-16 surrogates at the Rust scalar-string boundary; the pinned package export is near `packages/coding-agent/src/index.ts:281`.
- [x] Compare the skill-block parser through exactly 10 persistent differential requests (`13138..=13147`), with runtime public-index proof separated from direct-module exact-shape/mutability typeproof; current focused, differential, and oracle evidence is recorded in the [dedicated spec](skill-block.md) and [traceability](../porting/traceability/skill-block.md).

### Shared Date.parse fixtures

- [ ] Match observed numeric-string, named-month, slash and pre-1970 HTTP/asctime fixtures. Numeric cases cover `0`, months `1..12`, invalid `13..31`, year pivots `32..49` → 2032..2049 and `50..99` → 1950..1999, plus tested sign/padding/long-year/`Z` boundaries; strict ISO parsing retains precedence.
- [ ] Preserve tested month order, local/GMT/UTC/numeric-offset times, day normalization, invalid inputs and Chicago DST transitions. Named-month seconds `60`, `61`, `99` reset to zero; slash/ISO second `60` rejects.

Independent integrated proof is pending. The finite corpus does not establish general V8 grammar completeness; exact fixtures and development evidence live in [shared compatibility traceability](../porting/traceability/js-compat.md#dateparse-srcdaters).

### en-US local-date formatting

- [x] Pin `Date.prototype.toLocaleString()` date output to en-US regardless of host locale, retaining the process-local time zone (approved October 20, 2026, extending the en-US number-grouping deviation).
- [x] Format supported epoch milliseconds as `M/D/YYYY, h:mm:ss AM`/`PM`: unpadded month/day/hour, two-digit minutes/seconds, ASCII spaces including before AM/PM, matching pinned Node v26.11.1. Truncate subsecond precision.
- [x] Resolve named `TZ` zones with production zoneinfo data; compare fixed midnight/noon, AM/PM, single/double-digit and New York DST-boundary values in fresh UTC/New York processes. Return no value when the local zone cannot be resolved or the timestamp lies outside Jiff's supported calendar range; no UTC fallback.

Shared helper: `pi_js_compat::date::to_locale_string`. Behavioral/differential proof:
`crates/pi-coding-agent/tests/example_input_transform.rs`; mapping and bounded proof
limits: [extension batch B](../porting/traceability/extension-examples-b.md).
This slice does not claim all `Intl` options, arbitrary calendar ranges, or exhaustive time-zone parity.

## How it works

- [Differential port system](../wiki/systems/differential-port.md)

## Implementation inventory

- `docs/porting/source-lock.json` — pins the read-only TypeScript oracle.
- `oracle/` — invokes selected TypeScript source functions.
- `crates/*/tests/differential_*.rs` — compares Rust results with the oracle.
- `docs/porting/traceability/` — maps pinned source statements to proof.
- `crates/pi-js-compat/src/date.rs` — shared JavaScript date parsing and timezone behavior.

## Tests asserting this spec

- `crates/pi-js-compat/tests/date.rs`
- `crates/pi-js-compat/tests/differential_date_parse.rs`

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
- `crates/pi-coding-agent/tests/defaults.rs`
- `crates/pi-coding-agent/tests/differential_config_value_references.rs`
- `crates/pi-coding-agent/tests/differential_default_thinking_level.rs`
- `oracle/default-thinking-level-oracle.ts`
- `oracle/default-thinking-level-typecheck.mjs`
- `crates/pi-coding-agent/tests/custom_operation_errors.rs`
- `crates/pi-coding-agent/tests/example_ssh.rs`
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
- `crates/pi-agent-core/tests/thinking_level.rs`
- `crates/pi-agent-core/tests/differential_truncate/thinking_level.rs`
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
- Lone-surrogate input supplied directly to existing `String`-typed core APIs. Raw JSON file escapes are representable input, not this exclusion.
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
