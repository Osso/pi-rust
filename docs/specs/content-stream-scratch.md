# Valid content-stream scratch preservation

Preserve valid runtime output properties from pinned Anthropic Messages, shared OpenAI Responses consumers, and `packages/coding-agent/examples/extensions/custom-provider-anthropic/index.ts`. [Traceability](../porting/traceability/ai-types.md) records the bounded representation and producer proof.

## What it must do

- [x] Preserve optional numeric `index` on text, thinking, and tool-call content in complete event serde round trips, including the source example's `text_start` block with index 3.
- [x] Preserve optional tool `partialJson` verbatim; constructors leave both fields absent. Use the existing JS-number serialization convention for indices, without restricting them to integers.
- [x] Anthropic Messages start/delta snapshots retain indices and accumulated tool JSON; stop removes scratch before end snapshots.
- [x] Anthropic Messages errors before block stop remove scratch before error/result snapshots, retaining parsed arguments, text, thinking, usage, and source error ownership.
- [x] Compare full pinned-provider event content without filtering `index` or `partialJson`; retain documented timestamp and SDK identity proof boundaries.
- [x] Shared Responses tool starts retain initial `partialJson`; deltas accumulate and parse it; arguments.done replaces it and emits only an additional suffix; output_item.done removes it before tool end.
- [x] An unfinished tool retains its buffer on successful Responses completion; provider failures remove it before error/result in Responses, Azure and Codex.
- [x] Completions start/delta snapshots retain native `partialArgs` and binary64 `streamIndex`; stream finalization or provider error removes them before end/error snapshots. Fractional indices address distinct interleaved tools without integer narrowing.
- [x] Bedrock start/delta snapshots retain native binary64 index and tool partialJson; text/thinking starts precede content mutation, and block stop/error removes scratch before end/error snapshots.
- [x] Mistral valid tool start/delta snapshots retain native partialArgs; stream finalization and owner failure remove it before end/error snapshots.

## How it works

- [Shared content representation](../porting/traceability/ai-types.md)
- [Anthropic lifecycle and remaining proof boundaries](../porting/traceability/anthropic-messages.md)
- [Responses lifecycle and consumers](../porting/traceability/openai-responses-shared.md#native-tool-scratch-lifecycle)
- [Completions lifecycle](../porting/traceability/openai-completions.md#native-completions-scratch)
- [Bedrock lifecycle](../porting/traceability/bedrock-converse-stream.md#native-scratch-and-push-ordering)
- [Mistral native lifecycle and SDK proof boundaries](../porting/traceability/mistral-conversations.md#q4-common-pin-status)

## Implementation inventory

- `crates/pi-ai/src/types.rs`: optional content fields and constructor absence.
- `crates/pi-ai/src/api/anthropic_messages.rs`: source-matching scratch lifecycle.
- `crates/pi-ai/src/api/openai_responses_shared.rs`: native tool scratch authority and start/delta/done lifecycle.
- `crates/pi-ai/src/api/{openai_responses,azure_openai_responses,openai_codex_responses}.rs`: failure cleanup at the owning provider boundary.
- Existing workspace struct literals: mechanical `None` initialization only.

## Tests asserting this spec

- `crates/pi-ai/tests/types.rs`: complete valid-event serde round trips and absent-field defaults.
- `crates/pi-ai/tests/anthropic_messages.rs`: existing 27 adapter behavioral tests.
- `crates/pi-ai/tests/differential_anthropic_messages.rs`: 44 controlled-fetch/SSE source scenarios; three new open-block errors.
- `oracle/anthropic-messages-oracle.ts`, `crates/pi-ai/tests/support/anthropic.rs`: timestamp normalization only for event content; no scratch filtering.
- `crates/pi-ai/tests/differential_openai_responses{,_shared}.rs`: unmasked full events; five open-tool success/failure/arguments.done scenarios.
- Azure whole-provider differential and Codex `differential_open_tool_error_cleans_scratch_after_delta_snapshots`: full open-tool failure events and results.
- `crates/pi-ai/tests/differential_openai_completions.rs`:35 existing scenarios and four numeric-index/open-error/EOF/source-absent custom cases; no scratch filtering.

## Known gaps (current cycle)

- [ ] Provider behavior beyond the recorded matrices remains unproved. Source-absent customInput behavior is removed only where explicitly traced.
- [ ] Full native custom Anthropic example lifecycle is independently owned by its example slice; shared serde preservation does not establish producer parity.

## Out of scope

Other backend scratch unions, live provider traffic, SDK/reader visibility, and changes to timestamp/error ownership. Prior scratch-normalized comparisons did not prove strict valid-event parity; valid output fields are not typed-input exclusions.
