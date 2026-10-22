# Agent-core message traceability

Pinned source: `packages/agent/src/harness/messages.ts:4-17,19-29,40-45` and package export `packages/agent/src/index.ts:76` at commit `532f7d12cd19235081bd4d6e425bde185652d86f`.

| TypeScript source | Observable contract | Rust proof or gap |
|---|---|---|
| `messages.ts:4-7` | Compaction prefix is the exact 96-byte LF-only literal ending after `<summary>\n` | `harness::messages::COMPACTION_SUMMARY_PREFIX`; focused exact-value, length, and framing tests; direct package-index request `13096` |
| `messages.ts:9-10` | Compaction suffix is the exact 11-byte literal `\n</summary>` | `COMPACTION_SUMMARY_SUFFIX`; focused leading-LF and exact-value proof; request `13096` |
| `messages.ts:12-15` | Branch prefix is the exact 89-byte LF-only literal ending after `<summary>\n` | `BRANCH_SUMMARY_PREFIX`; focused exact-value, length, and framing tests; request `13096` |
| `messages.ts:17` | Branch suffix is the exact 10-byte literal `</summary>` without a leading LF | `BRANCH_SUMMARY_SUFFIX`; focused no-leading-LF and exact-value proof; request `13096` |
| `index.ts:76` | `export * from "./harness/messages.ts"` exposes all four runtime constants at package root | Rust re-exports all four from `pi_agent_core::harness`; focused imports prove module and harness-root paths; request `13096` compares runtime exports and `public-index-typecheck.mjs` proves exact literal types |
| `messages.ts:19-29` | `BashExecutionMessage` is a public mutable value with role literal `"bashExecution"`, two string fields, required `exitCode: number | undefined`, two booleans, optional `fullOutputPath`, required numeric `timestamp`, and optional `excludeFromContext` | `harness::messages::BashExecutionMessage`; exact field types map to `BashExecutionRole`, `String`, `Option<f64>`, `bool`, `Option<String>`, `f64`, and `Option<bool>`; focused role, mutation, optional-state, string-boundary, and binary64-bit proof; direct plain-record groups `13097..=13104` |
| `index.ts:76` | The package-wide message export includes `BashExecutionMessage` | Re-exported from `pi_agent_core::harness::messages` and `pi_agent_core::harness`; focused imports pass; strict public-index exact-shape/mutability/number proof passes |
| `messages.ts:40-45` | `BranchSummaryMessage` is a public mutable value with role literal `"branchSummary"`, `summary`, `fromId`, and unrestricted numeric `timestamp` | `harness::messages::BranchSummaryMessage`; fields map to `BranchSummaryRole`, `String`, `String`, and `f64`; focused role, mutation, string-boundary, and binary64-bit proof; direct plain-record groups `13105..=13112` |
| `messages.ts:56-66` | `createBranchSummaryMessage(summary, fromId, timestamp)` returns a custom `branchSummary` message with role, summary, fromId, and numeric timestamp | `harness::messages::create_branch_summary_message`; `session_context.rs` asserts the exact JSON shape; session differential context scenarios compare against the TS constructor |
| `messages.ts:68-92` | `createCompactionSummaryMessage(summary, tokensBefore, timestamp, options)` emits role, summary, tokensBefore, optional tokensAfter/keptFromPreviousContextTokens/compactedResultTokens/durationMs/providerNative, then timestamp | `harness::messages::create_compaction_summary_message` + `CompactionSummaryOptions`; `session_context.rs` exact JSON; context differential |
| `index.ts:76` | The package-wide message export includes `BranchSummaryMessage` | Re-exported from `pi_agent_core::harness::messages` and `pi_agent_core::harness`; focused imports pass; strict public-index exact-shape/mutability/number proof passes |
| `messages.ts:31-39` | `CustomMessage<T>`: role `custom`, `customType`, string-or-block content, `display`, optional `details`, numeric timestamp | Custom messages remain `AgentMessage::Custom` JSON records; `CustomMessageContent` models `string \| (TextContent \| ImageContent)[]`; `create_custom_message` builds the record. Focused and differential custom-message fixtures |
| `messages.ts:48-54` | `CompactionSummaryMessage` optional tokens/providerNative fields | Custom JSON record; `create_compaction_summary_message` field order; session-context proof and conversion fixtures |
| `messages.ts:68-92` | `createBranchSummaryMessage`/`createCompactionSummaryMessage` | Existing builders; `session_context.rs` and differential context scenarios |
| `messages.ts:94-110` | `bashExecutionToText`: command, fenced output or `(no output)`, cancelled vs non-zero exit, truncation note only with a non-empty path | `bash_execution_to_text`; 13 focused cases and differential `bash_execution_to_text` fixtures `14100..=14112` |
| `messages.ts:111-119` | `createCustomMessage` emits role, customType, content, display, optional details, timestamp | `create_custom_message`; focused test and differential `create_custom_message` fixtures `14300..=14303` |
| `messages.ts:121-176` | `convertToLlm`: drops excluded bash messages and unknown custom roles, maps bash/custom/branch/compaction custom roles to user messages, passes user/assistant/toolResult through, preserves providerNative and timestamps | `convert_to_llm`; focused test and differential `convert_to_llm` fixture `14200` |

## Rust boundary

- The constants are `pub const &str` values in `pi_agent_core::harness::messages` and are re-exported from `pi_agent_core::harness`.
- TypeScript infers exact string-literal types; Rust represents the same immutable compile-time values directly rather than adding a wrapper type.
- Values are ASCII and LF-only. No CR, trailing spaces, runtime interpolation, escaping, trimming, or normalization is part of the contract.

## BashExecutionMessage boundary

- Closed Rust role enum: `BashExecutionRole::BashExecution`; exact `as_str()` value is `"bashExecution"`.
- Public mutable `BashExecutionMessage` derives `Debug` only; no constructors, serde, validation, or additional traits.
- Source `exitCode: undefined` becomes `None`; absent or explicit `undefined` optional values become `None`.
- Numeric fields use unrestricted `f64` storage to preserve binary64 bit patterns without validation or normalization.
- Invalid JavaScript runtime shapes and lone-surrogate strings remain outside the typed Rust boundary.

## BranchSummaryMessage boundary

- Closed Rust role enum: `BranchSummaryRole::BranchSummary`; exact `as_str()` value is `"branchSummary"`.
- Public mutable `BranchSummaryMessage` derives `Debug` only; no constructors, Date parsing, serde, validation, or additional traits.
- Numeric timestamp uses unrestricted `f64` storage to preserve binary64 bit patterns without validation or normalization.
- Invalid JavaScript runtime shapes and lone-surrogate strings remain outside the typed Rust boundary.

## Proof status

- BranchSummaryMessage focused RED: `/tmp/pi-rust-branch-summary-message-red.out` records exit 101 with only the missing module/root value-type imports.
- BranchSummaryMessage focused GREEN: `/tmp/pi-rust-branch-summary-message-green.out` records exit 0 with 4 passed and 0 failed; production commit `8e32578` adds the value types and focused proof.
- BranchSummaryMessage differential RED: `/tmp/pi-rust-branch-summary-message-differential-red.out` records all prior requests through `13104` passing before request `13105` reaches the unchanged oracle's unsupported operation and returns non-JSON.
- BranchSummaryMessage differential GREEN: `/tmp/pi-rust-branch-summary-message-differential-green.out` records exit 0 with one persistent test passing exactly eight request groups (`13105..=13112`); differential commit `fa8f6f2` adds the plain-record comparisons and public-index proof wiring.
- BranchSummaryMessage oracle/public-index typecheck: `/tmp/pi-rust-branch-summary-message-oracle.out` records `./run-tests.sh oracle` exit 0 with exact field-set, role-literal, mutability, and unrestricted-number proof.
- Readability follow-up: `0fad1f2` (`test(agent): simplify branch summary fixtures`) resolves both Rust test-fixture findings; `a991812` normalizes the remaining hexadecimal grouping for Clippy. Rust, TypeScript, and JavaScript audits report no in-scope violations.
- Final gates at `a991812`: 226 workspace tests passed with 0 failures and 2 ignored; workspace all-target check, all-target/all-feature Clippy with denied warnings, formatting, documentation audit, exact pinned source HEAD, clean worktrees, and commit-range diff checks passed.
- Inherited verification completed with `OVERALL: PASS` for the bounded branch-summary value slice at `4891655`.
- BashExecutionMessage focused RED: `/tmp/pi-rust-bash-execution-message-red.out` records exit 101 with only the missing module/root value-type imports.
- BashExecutionMessage focused GREEN: `/tmp/pi-rust-bash-execution-message-green.out` records exit 0 with 5 passed and 0 failed; production commit `143c635` adds the value types and focused proof.
- BashExecutionMessage differential RED: `/tmp/pi-rust-bash-execution-message-differential-red.out` records all prior requests through `13096` passing before request `13097` reaches the unchanged oracle's unsupported operation and returns non-JSON.
- BashExecutionMessage differential GREEN: `/tmp/pi-rust-bash-execution-message-differential-green.out` records exit 0 with one persistent test passing exactly eight request groups (`13097..=13104`); differential commit `0fa04e1` adds the plain-record comparisons and public-index proof wiring.
- BashExecutionMessage oracle/public-index typecheck: `/tmp/pi-rust-bash-execution-message-oracle.out` records `./run-tests.sh oracle` exit 0 with exact field-set, role-literal, mutability, required/optional, explicit-undefined, and unrestricted-number proof.
- Readability follow-up: `07b6557` (`test(agent): clarify bash message oracle`) resolves the only TypeScript dense-expression finding; Rust, TypeScript, and JavaScript re-audits report no in-scope violations.
- Final gates at `07b6557`: 222 workspace tests passed with 0 failures and 2 ignored; workspace all-target check, all-target/all-feature Clippy with denied warnings, formatting, documentation audit, exact pinned source HEAD, clean worktrees, and commit-range diff checks passed.
- Inherited verification completed with `OVERALL: PASS` for the bounded value slice at `60499e0`.
- Focused RED: `/tmp/pi-rust-summary-markers-red.out` records exit 101 with only the missing `harness::messages` module and harness-root constants.
- Focused GREEN: `/tmp/pi-rust-summary-markers-green.out` records exit 0 with 4 passed and 0 failed; production commit `6f3a6cc` adds the exact module/root constants and focused proof.
- Differential RED: `/tmp/pi-rust-summary-markers-differential-red.out` records all prior requests through `13095` passing before request `13096` reaches the unchanged oracle's unsupported operation and returns non-JSON.
- Differential GREEN: `/tmp/pi-rust-summary-markers-differential-green.out` records exit 0 with the persistent differential test passing request `13096`; differential commit `edd44b6` adds the Rust fixture and oracle operation.
- Oracle/public-index typecheck: `/tmp/pi-rust-summary-markers-oracle.out` records `./run-tests.sh oracle` exit 0 with runtime package-index export and exact literal-type proof.
- Final gates at `e4cde92`: 217 workspace tests passed with 0 failures and 2 ignored; workspace all-target check, all-target/all-feature Clippy with denied warnings, formatting, Rust/TypeScript/JavaScript readability, documentation audit, exact pinned source HEAD, clean worktrees, and commit-range diff checks passed.
- Inherited verification completed with `OVERALL: PASS` for the bounded slice at `010bce2`.

## Exclusions

Date or timestamp-string parsing (`new Date(value).getTime()`) is outside the typed boundary; callers pass milliseconds.
`convert_to_llm` is infallible like the source; stored custom fields outside the typed boundary read as empty
strings/`false`/`0`, and content blocks that do not deserialize as text/image are dropped. Session/events, compaction/UI/RPC behavior, dynamic formatting, fallbacks, and dependencies remain out of scope.

## Proof

- Focused: `tests/harness_messages_conversion.rs` (4 tests).
- Differential: `oracle/harness-core-oracle.ts` operations `bash_execution_to_text`, `convert_to_llm`,
  `create_custom_message`; fixtures `14100..=14112`, `14200`, `14300..=14303`.

## Session-context constructor extension

`session/context.ts` needs `createBranchSummaryMessage` and `createCompactionSummaryMessage`. The Rust port adds both to
`harness/messages.rs` as `AgentMessage::Custom` builders using the same field order and JS-number formatting.
String timestamps (`new Date(value).getTime()`) are excluded by the typed boundary; callers pass `i64` milliseconds.

## Conversion helpers (`convertToLlm` / `bashExecutionToText`)

Added for `harness/compaction`; tested by `messages_convert.rs` and the
`serialize_conversation` differential cases.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `messages.ts:68-84` | `bashExecutionToText` renders `Ran \`cmd\``, fenced output or `(no output)`, then cancelled / non-zero exit / truncated-output markers | `bash_execution_to_text`; exact-output cases for all branches in `messages_convert.rs` |
| `messages.ts:150-187` | `convertToLlm` maps bash execution (skipped when `excludeFromContext` is truthy), custom string/block content, branch and compaction summaries to user messages; passes standard roles through; drops unknown roles | `convert_to_llm`; conversion cases in `messages_convert.rs`; differential `serialize_conversation` covers custom, bash, branch, and compaction messages |
| `messages.ts:130-136` | Compaction summary conversion carries `providerNative` | `compaction_summary_message` deserializes `ProviderNativeContext` |

### Conversion boundary

- Custom messages are raw JSON objects. `excludeFromContext` uses JavaScript truthiness; unknown block types and malformed payloads are dropped by the typed `UserContent` boundary.
- Bash execution `exitCode` uses `Option<f64>`; a missing value suppresses the exit marker, matching `null`/`undefined` in the source.
- Custom/branch/compaction timestamps are read as JS numbers and narrowed to `i64` milliseconds; string timestamps stay excluded.
- Numeric `bashExecutionToText` exit codes use `f64` display, which matches JS integer rendering for real exit codes.

### Conversion proof status

- Focused: `messages_convert.rs` 7 passed, 0 failed.
- Differential: `differential_compaction.rs` compares `serializeConversation(convertToLlm(...))` output for custom, bash, branch, compaction, and tool-result messages against the pinned source.
- Gates: `cargo fmt --check`, targeted `cargo test -p pi-agent-core`, and `cargo clippy -p pi-agent-core --all-targets -- -D warnings` pass on branch `port/harness-compaction`.

Q18 coupled native helper contracts: create_custom_message accepts &JsString customType; create_branch_summary_message accepts &JsString fromId; BranchSummaryMessage.from_id is JsString. `pi-agent-core/tests/lossless_provider_errors.rs::custom_type_and_branch_origin_keep_native_units` exercises serialized raw code units (compiler RED on prior scalar helper). Existing scalar session metadata callers convert losslessly into the native helper; no raw wrapper path.
