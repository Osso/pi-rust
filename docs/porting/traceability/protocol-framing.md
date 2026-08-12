# Protocol-framing traceability

Pinned source: `packages/protocol/src/framing.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

| TypeScript source | Observable behavior | Rust proof |
|---|---|---|
| `framing.ts:1-6` | Four-byte header, unsigned 32-bit limit, 16 MiB default | Pending |
| `framing.ts:12-16` | Framing failures identify as `FrameError` | Pending normalized error proof |
| `framing.ts:19-25` | Default and valid configured limits | Pending for representable `Option<u32>` values |
| `framing.ts:29` | Reject non-`Uint8Array` payloads | Approved exclusion: typed Rust input |
| `framing.ts:30` | Reject payloads above `u32::MAX` | Pending; direct allocation proof is impractical |
| `framing.ts:31-36` | Allocate frame and write length big-endian | Pending |
| `framing.ts:37-38` | Copy payload and return owned frame | Pending |
| `framing.ts:43` | Reject non-`Uint8Array` frames | Approved exclusion: typed Rust input |
| `framing.ts:44` | Reject fewer than four header bytes | Pending |
| `framing.ts:45` | Decode unsigned 32-bit big-endian length | Pending |
| `framing.ts:46-49` | Apply configured maximum before size equality | Pending |
| `framing.ts:50-52` | Require exactly one complete payload | Pending |

`FrameDecoder` begins at `framing.ts:55` and is not part of this slice.
