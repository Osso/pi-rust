# Lossless shared message content (Q18)

Approved 2026-10-21: shared JS message text and arbitrary JSON properties retain exact UTF-16 as single authorities. Frozen source: `4f9e6f8a4`; [implementation and proof](../porting/traceability/lossless-content.md).

## What it must do

- [x] `TextContent.text` preserves lone surrogates through shared JSON serialization/deserialization.
- [x] `ToolResultMessage.details` preserves arbitrary nested UTF-16 strings/keys and distinguishes omission from explicit null.
- [x] `ProviderNativeContext.value` is a lossless JSON authority, including native request items passed through Responses APIs.
- [x] Pi-messages HTTP context and `OnPayload` replacements serialize unmatched units as escapes, matching Node `JSON.stringify`.
- [x] Explicit provider `sanitizeSurrogates` sites remove unmatched units, retaining valid pairs; no mutation of shared text precedes that boundary.
- [x] Shared Node UTF-8 boundary replaces unmatched units with U+FFFD only when encoding to streams, filesystem paths, process arguments or environment values.
- [x] Native environment names remain String; values remain JsString until the OS boundary. Lone-surrogate names are a typed exclusion, not a value-text exclusion. Invalid names never create or alias entries; Node first-NUL prefix aliasing is excluded by the approved runtime-oddity rule ([proof](../porting/traceability/process-env.md)). Human labels retain their separate UTF-16 authority.
- [x] User and thinking text use the same UTF-16 authority.
- [x] Core custom-message text uses the same UTF-16 authority; coding-agent consumers remain pending.
- [x] Plan-mode generated todo text retains split surrogate units through status/widgets, progress and execution/completion messages, JSONL persistence and restoration ([proof](../porting/traceability/example-plan-mode.md)).
- [x] Tool-call arguments and complete/partial JSON argument parsing use lossless object/value authorities.
- [x] Core `AgentTool` and coding extension/context definitions retain native UTF-16 label/description through provider context and definition conversion.
- [x] Human `Tool.description` preserves unmatched UTF-16 units through all nine chat API request bodies, whole-Tool serialization and token estimators; it is not opaque scalar metadata.
- [x] Anthropic SSE retains text/thinking and native UTF-16 JSON fragments through ordered block/delta/stop handling.
- [x] User classification (2026-10-21): “Approve scalar metadata gap” for opaque provider IDs/names, type/stop tags and signatures. Unpaired units must cause explicit errors, never replacement; arbitrary argument keys named `id`/`name`/`type` and human text/errors are not covered.
- [ ] Input values, UTF-16 cursor positions, inside-pair edits and rendered lines must match frozen source. User chose “Widen Input to UTF-16” (2026-10-21), not a representation gap.
- [x] Core execution/hook parameters use those same authorities; coding-agent tool wrappers remain pending.
- [ ] Agent result text/details survive agent loop, JSONL append/reload and TS reread in both directions.
- [ ] Question relay, ask-questions, mailbox and session-name tools preserve arbitrary UTF-16.
- [x] Incoming control-message producer and claimed/delivered content use native UTF-16. SQLite TEXT encoding alone applies Node replacement; completion/failure transitions must not mutate the caller's text.
- [ ] Coding tool caps preserve retained UTF-16 lines/prefixes; only the frozen Buffer suffix boundary decodes encoded bytes.

## How it works

- [Shared serde contract](js-json.md#shared-serde-q18-2026-10-21).
- [Boundary and proof mapping](../porting/traceability/lossless-content.md).

## Implementation inventory

- `crates/pi-js-compat/src/js_json.rs` — UTF-16 authorities, JSON serde and explicitly named external encoding boundary.
- `crates/pi-ai/src/types.rs` — shared text/tool-result fields and lossless union decoding.
- `crates/pi-ai/src/request.rs` — lossless payload callback authority.
- `crates/pi-ai/src/api/` — per-source provider sanitation and callback-to-HTTP JSON transport.
- `crates/pi-ai/src/utils/text.rs` — lossless content extraction.

## Tests asserting this spec

- `crates/pi-js-compat/tests/js_json_serde.rs`.
- `crates/pi-ai/tests/lossless_content.rs`.
- `crates/pi-ai/tests/tool_description.rs` — actual frozen request descriptions for all nine chat APIs.
- `crates/pi-ai/tests/tool_metadata.rs` — whole-Tool JSON, Faux and context estimates including unmatched description units.
- `crates/pi-agent-core/tests/lossless_agent.rs`.
- `crates/pi-coding-agent/tests/lossless_session.rs` and `lossless_tools.rs` (blocked pending coding-agent compilation).
- `crates/pi-coding-agent/tests/lossless_incoming_content.rs` — native DTO/producer/claim and real SQLite/frozen source boundary.
- `oracle/lossless-content-oracle.ts` — frozen sanitizer plus Node JSON/string-unit inspection.

## Known gaps (current cycle)

- [ ] Downstream compilation, broad affected targets, persisted JSONL proof, session-name runtime integration and final warning-free workspace check remain open. All new parent `lossless_session` tests are compiler-blocked, not runtime PASS.
- [ ] Raw UI text must survive component rendering/composition to the actual terminal write. Scalar `Vec<String>` render lines and premature Text encoding remain unapproved gaps; raw RPC/notification queues alone do not establish lossless UI.
- [ ] Human provider errors have a bounded native transport proof, but downstream error consumers, human stop explanations and non-stream image JSON remain open. Opaque scalar protocol metadata is an approved gap with explicit rejection; it does not waive broader ingress validation. Merged ingress `5909f149` has six sandbox SSE/Codex-WebSocket tests PASS; runtime `e3ce34c3` and bundled `553ed154` have compiler-blocked tests, not runtime acceptance. See the [bounded integration checkpoints](../porting/traceability/lossless-content.md#merged-lane-checkpoints); no Q18/full-port acceptance.

## Out of scope

Frozen source is immutable. No parallel raw/typed copies or lossy fallbacks are allowed. Ordinary serde formats cannot express lone-surrogate strings; raw JSON serde is required for lossless decoding.
