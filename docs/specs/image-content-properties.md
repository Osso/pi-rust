# Image content properties

Frozen `ai/src/types.ts:363–367` declares `type`, `data`, and `mimeType`. Message and session paths retain the image object, including unknown properties.

## Requirements

- Store the known fields once in typed `ImageContent` fields. Reuse the public `ModelProperties` decode/serialization/order authority for unknown properties and original key order.
- Preserve unknown nested keys and values with exact UTF-16 units. Preserve native nonfinite numbers until JSON serialization, which emits null.
- Serialize current known-field values after mutation in their original positions, without stale duplicate values.
- Preserve images through RPC prompt/get_messages and bidirectional TypeScript/native session append/load.
- Do not forward unknown image properties through explicit provider payload mappings.

## Proof

- `crates/pi-ai/tests/image_content_properties.rs`: native/JSON order, mutation, nested UTF-16, Infinity and actual frozen Completions mapping negative control.
- `crates/pi-coding-agent/tests/image_content_roundtrip.rs`: real frozen RPC process and session file paths in both directions.
- `crates/pi-coding-agent/tests/differential_rpc_mode.rs`: replaces the obsolete unknown-key rejection with frozen-matching prompt/get_messages acceptance; known-field scalar rejection remains.

## Boundaries

Existing typed runtime-input exclusions and opaque-scalar rejection for image data/mimeType remain unchanged. No provider protocol behavior, dependencies or fallback representation is added. [Carrier authority](model-json-properties.md), [RPC mapping](../porting/traceability/rpc-mode.md), [AI type mapping](../porting/traceability/ai-types.md).
