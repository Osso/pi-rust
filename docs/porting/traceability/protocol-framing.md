# Protocol-framing traceability

Pinned source: `packages/protocol/src/framing.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Differential proof: `differential_framing.rs::framing_matches_the_pinned_typescript_oracle` executes the real source for all representable fixtures listed below.

| TypeScript source | Observable behavior | Rust proof |
|---|---|---|
| `framing.ts:1-6` | Four-byte header, unsigned 32-bit limit, 16 MiB default | Focused prefix/default tests; differential ordinary/default-limit fixtures |
| `framing.ts:12-16` | Framing failures identify as `FrameError` | Focused message test; differential validation-error fixtures |
| `framing.ts:19-25` | Default and valid configured limits | Focused default and `u32::MAX` tests; differential exact/configured/default-limit fixtures |
| `framing.ts:29` | Reject non-`Uint8Array` payloads | Approved exclusion: typed Rust input |
| `framing.ts:30` | Reject payloads above `u32::MAX` | Explicit proof gap: allocating a greater-than-4-GiB slice is impractical |
| `framing.ts:31-36` | Allocate frame and write length big-endian | Focused ordinary/high-order tests; differential encode fixtures |
| `framing.ts:37-38` | Copy payload and return owned frame | Focused owned-frame test; differential encoded values |
| `framing.ts:43` | Reject non-`Uint8Array` frames | Approved exclusion: typed Rust input |
| `framing.ts:44` | Reject fewer than four header bytes | Focused short-prefix test; differential short-header fixture |
| `framing.ts:45` | Decode unsigned 32-bit big-endian length | Focused high-order decode test; differential configured/default-limit fixtures |
| `framing.ts:46-49` | Apply configured maximum before size equality | Focused limit tests; differential configured/default-limit fixtures |
| `framing.ts:50-52` | Require exactly one complete payload | Focused exact/partial/trailing tests; matching differential fixtures |

`FrameDecoder` begins at `framing.ts:55` and is not part of this slice.
