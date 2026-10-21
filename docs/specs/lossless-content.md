# Lossless shared message content (Q18)

Approved 2026-10-21: shared JS message text and arbitrary JSON properties retain exact UTF-16 as single authorities. Frozen source: `4f9e6f8a4`; [implementation and proof](../porting/traceability/lossless-content.md).

## What it must do

- [x] `TextContent.text` preserves lone surrogates through shared JSON serialization/deserialization.
- [x] `ToolResultMessage.details` preserves arbitrary nested UTF-16 strings/keys and distinguishes omission from explicit null.
- [x] Pi-messages HTTP context and `OnPayload` replacements serialize unmatched units as escapes, matching Node `JSON.stringify`.
- [x] Explicit provider `sanitizeSurrogates` sites remove unmatched units, retaining valid pairs; no mutation of shared text precedes that boundary.
- [x] Shared Node UTF-8 boundary replaces unmatched units with U+FFFD only when encoding to streams, filesystem paths, process arguments or environment values.
- [ ] User, thinking and custom-message text use the same UTF-16 authority.
- [ ] Tool-call arguments and execution/hook parameters use lossless object/value authorities.
- [ ] Agent result text/details survive agent loop, JSONL append/reload and TS reread in both directions.
- [ ] Question relay, ask-questions, mailbox and session-name tools preserve arbitrary UTF-16.

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
- `oracle/lossless-content-oracle.ts` — frozen sanitizer plus Node JSON/string-unit inspection.

## Known gaps (current cycle)

- [ ] This is a pi-ai crate-compiling API checkpoint, not workspace readiness; downstream consumers, broad affected targets, agent/JSONL proof, session-name integration and final warning-free workspace check remain open.

## Out of scope

Frozen source is immutable. No parallel raw/typed copies or lossy fallbacks are allowed. Ordinary serde formats cannot express lone-surrogate strings; raw JSON serde is required for lossless decoding.
