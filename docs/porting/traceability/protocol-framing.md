# Protocol-framing traceability

Pinned source: `packages/protocol/src/framing.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

| TypeScript source | Observable behavior | Rust proof |
|---|---|---|
| `framing.ts:1-6` | Four-byte header, unsigned 32-bit limit, 16 MiB default | `framing.rs::prefixes_payload_with_unsigned_big_endian_length`; `uses_the_default_maximum_frame_length` |
| `framing.ts:12-16` | Framing failures identify as `FrameError` | `framing.rs::frame_errors_preserve_source_messages`; normalized differential kind pending |
| `framing.ts:19-25` | Default and valid configured limits | `framing.rs::uses_the_default_maximum_frame_length`; `accepts_the_unsigned_32_bit_maximum_as_a_limit` |
| `framing.ts:29` | Reject non-`Uint8Array` payloads | Approved exclusion: typed Rust input |
| `framing.ts:30` | Reject payloads above `u32::MAX` | Explicit proof gap: allocating a greater-than-4-GiB slice is impractical |
| `framing.ts:31-36` | Allocate frame and write length big-endian | `framing.rs::prefixes_payload_with_unsigned_big_endian_length`; `writes_nonzero_high_order_length_bytes` |
| `framing.ts:37-38` | Copy payload and return owned frame | `framing.rs::returns_owned_frame_bytes` |
| `framing.ts:43` | Reject non-`Uint8Array` frames | Approved exclusion: typed Rust input |
| `framing.ts:44` | Reject fewer than four header bytes | `framing.rs::rejects_incomplete_length_prefix` |
| `framing.ts:45` | Decode unsigned 32-bit big-endian length | `framing.rs::decodes_nonzero_high_order_length_bytes` |
| `framing.ts:46-49` | Apply configured maximum before size equality | `framing.rs::rejects_declared_length_above_configured_limit`; `uses_the_default_maximum_frame_length` |
| `framing.ts:50-52` | Require exactly one complete payload | `framing.rs::validates_exactly_one_complete_bounded_frame`; `rejects_partial_payload_and_trailing_bytes` |

`FrameDecoder` begins at `framing.ts:55` and is not part of this slice.
