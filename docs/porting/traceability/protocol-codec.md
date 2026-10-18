# Protocol-codec traceability

Pinned source: `packages/protocol/src/codec.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Rust: `crates/pi-protocol/src/codec.rs` with `src/codec/{parse,serialize}.rs`, re-exported from `pi_protocol`.

Behavioral proof: `crates/pi-protocol/tests/codec.rs` ports `packages/protocol/test/protocol.test.ts` (validation, handshake, transcript states, framed encode/decode, failure terminality). `crates/pi-protocol/tests/schemas_roundtrip.rs` round-trips every command, command result, server event, and server message variant through the public codec.

Differential proof: `crates/pi-protocol/tests/differential_protocol/codec.rs` encodes concrete client/server messages through both the real pinned `encodeClientMessage`/`encodeServerMessage` and the Rust codec and compares framed bytes exactly; it also compares `parseClientMessage`/`parseServerMessage` acceptance and rejection. The existing schema suites compare acceptance and normalized values against the real pinned TypeBox schemas. CBOR value parity is proved separately in `protocol-cbor.md`.

| TypeScript source | Observable behavior | Rust proof |
|---|---|---|
| `codec.ts:18-22` | `ProtocolValidationError` identifies validation/framing failures | `ProtocolValidationError`; failure-message assertions in codec tests |
| `codec.ts:25-38` | Protocol values reject `undefined` outside optional properties, cycles, non-plain objects, and invalid nested values | Typed `CborValue`/`JsonValue` exclude cycles and non-plain objects; byte strings reject; `rejects_cbor_byte_strings_in_protocol_values` |
| `codec.ts:41-46` | `parseClientMessage` rejects invalid values with one generic message | `parse_client_message`; every client rejection test |
| `codec.ts:48-53` | `parseServerMessage` rejects invalid values with one generic message | `parse_server_message`; every server rejection test |
| `codec.ts:55-58` | Error messages are bounded to 500 characters | `bounded_message`; error-wrapping paths in decoder tests |
| `codec.ts:60-75` | Validate before encode, apply frame limit to CBOR, frame, and re-check one complete frame; wrap failures | `encode_message`/`encode_message_inner`; outbound-limit and round-trip tests |
| `codec.ts:77-85` | Encode one client/server message | `encode_client_message`, `encode_server_message`; all round-trip tests |
| `codec.ts:88-127` | Incremental validated decoder: terminal failed state, frame/CBOR/parse failures, bounded messages | `ValidatedMessageDecoder`; fragmented, invalid-input, terminal, truncated, oversized tests |
| `codec.ts:129-159` | Client and server decoder wrappers | `ClientMessageDecoder`, `ServerMessageDecoder`; all framed tests |
| `codec.ts:162-168` | Factory functions | `create_client_message_decoder`, `create_server_message_decoder`; factory test |
| `codec.ts:170-172` | Supported version predicate | `is_supported_protocol_version`; protocol-version test |

## Parse mapping (`src/codec/parse.rs`)

Each TypeScript `Check(Schema, value)` call maps to one strict Rust parser. Parsers reject unknown fields, missing required fields, wrong runtime types, empty IDs where `minLength: 1` applies, and invalid literals.

| Schema group | Rust parser | Proof |
|---|---|---|
| `ClientHelloSchema`, `RequestEnvelopeSchema`, `ClientMessageSchema` | `client_hello`, `request_envelope`, `client_message` | Codec handshake/request tests; differential message-envelope suites |
| `CommandSchema` and nine leaves | `command` and leaf helpers | `round_trips_every_client_command`; differential command suites |
| `ThinkingLevelSchema`, `SessionPhaseSchema`, `ProtocolErrorCodeSchema` | `thinking_level`, `session_phase`, protocol-code parse via `FromStr` | Existing schema-leaf and round-trip tests |
| `ModelRefSchema`, `ModelCostSchema`, `ModelMetadataSchema` | `model_ref`, `model_cost`, `model_metadata` | Codec/round-trip tests; differential model suites |
| `UsageSchema` | `usage`, `usage_cost` | Codec/round-trip tests; differential usage suite |
| `TextContentSchema`, `ThinkingContentSchema`, `ImageContentSchema`, `ToolCallContentSchema` | `text_content`, `thinking_content`, `image_content`, `tool_call_content` | Codec and round-trip tests |
| `UserContentSchema`, `AssistantContentSchema`, `ToolContentSchema` | `user_content`, `assistant_content`, `tool_content` | Round-trip transcript variants |
| `UserTranscriptItemSchema`, `AssistantTranscriptItemSchema`, `ToolTranscriptItemSchema`, `TranscriptItemSchema` | `user_transcript_item`, `assistant_transcript_item`, `tool_transcript_item`, `transcript_item` | Consistent/inconsistent state tests; full snapshot round-trip |
| `TranscriptProgressSchema` | `transcript_progress` | Round-trip every progress variant; nonterminal-finished tests |
| `SessionMetadataSchema`, `SessionSnapshotSchema`, `ServerSnapshotSchema` | `session_metadata`, `session_snapshot`, `server_snapshot` | List/session tests; full snapshot round-trip |
| `ProtocolErrorSchema` | `protocol_error` | Error-response round-trip |
| `CommandResultSchema` and nine results | `command_result` | `round_trips_every_command_result_variant` |
| `ServerEventSchema`, `ServerHelloSchema`, `ServerHelloErrorSchema`, `ResponseEnvelopeSchema`, `EventEnvelopeSchema`, `ServerMessageSchema` | `server_event`, `server_hello`, `server_hello_error`, `response_envelope`, `event_envelope`, `server_message` | `round_trips_every_server_message_and_event_variant`; invalid-server-message tests |
| `JsonValueSchema` | `json_value` | Nested tool-details test; byte-string rejection |

## Serialization mapping (`src/codec/serialize.rs`)

Every typed record serializes to a `CborValue::Map`. Optional `None` fields are omitted; `Some` values are emitted. Map key order is the pinned TypeScript producer's insertion order, because the CBOR encoder preserves it. Every record except `AssistantTranscriptItem` has a single producer whose order equals the schema declaration order. `AssistantTranscriptItem` records which producer created it (`AssistantTranscriptOrder::Bridge` for `toProtocolAssistantMessage`, `Runtime` for `TestSessionRuntime.prompt`) because those two pinned producers order assistant fields differently. `tests/differential_protocol.rs` and `tests/differential_stream.rs` compare raw frame bytes, including map order.

Parsing a received frame collapses assistant wire order to `AssistantTranscriptOrder::Bridge`; no Rust path re-encodes a parsed message, so this is unobservable.

## Approved typed-core exclusions

- `parseClientMessage`/`parseServerMessage` accept `unknown`; Rust accepts the typed `CborValue` produced by `decode_cbor`. `undefined`, cycles, functions, symbols, bigint, Date, Map, and non-plain objects are unrepresentable.
- `encodeClientMessage`/`encodeServerMessage` re-parse their input; Rust messages are already validated by construction, so encoding cannot fail validation. CBOR/framing failures still wrap with the same `Unable to encode … protocol message` prefix.
- TypeScript rejects fractional, negative, NaN, and oversized `maxFrameLength` at runtime; Rust `Option<u32>` makes them unrepresentable.
- `isSupportedProtocolVersion` takes a JavaScript `number` and rejects fractions; Rust takes `u64`, so fractions are unrepresentable.
- TypeBox integers above the safe JavaScript range are excluded by the CBOR decoder before parsing; Rust `u64` fields additionally exclude values above `u64::MAX`.
