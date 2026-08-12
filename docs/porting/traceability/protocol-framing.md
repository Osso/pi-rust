# Protocol-framing traceability

Pinned source: `packages/protocol/src/framing.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Differential proof: `differential_framing.rs::framing_matches_the_pinned_typescript_oracle` executes the real source for all representable fixtures listed below.

| TypeScript source | Observable behavior | Rust proof |
|---|---|---|
| `framing.ts:1-6` | Four-byte header, unsigned 32-bit limit, 16 MiB default | Focused prefix/default tests; differential ordinary/default-limit fixtures |
| `framing.ts:11-16` | Framing failures identify as `FrameError` | Focused message test; differential validation-error fixtures |
| `framing.ts:18-23` | Default and valid configured limits | Focused default and `u32::MAX` tests; differential exact/configured/default-limit fixtures |
| `framing.ts:27` | Reject non-`Uint8Array` payloads | Approved exclusion: typed Rust input |
| `framing.ts:28` | Reject payloads above `u32::MAX` | Explicit proof gap: allocating a greater-than-4-GiB slice is impractical |
| `framing.ts:29-34` | Allocate frame and write length big-endian | Focused ordinary/high-order tests; differential encode fixtures |
| `framing.ts:35-36` | Copy payload and return owned frame | Focused owned-frame test; differential encoded values |
| `framing.ts:40` | Reject non-`Uint8Array` frames | Approved exclusion: typed Rust input |
| `framing.ts:41` | Reject fewer than four header bytes | Focused short-prefix test; differential short-header fixture |
| `framing.ts:42` | Decode unsigned 32-bit big-endian length | Focused high-order decode test; differential configured/default-limit fixtures |
| `framing.ts:43-46` | Apply configured maximum before size equality | Focused limit tests; differential configured/default-limit fixtures |
| `framing.ts:47-49` | Require exactly one complete payload | Focused exact/partial/trailing tests; matching differential fixtures |

`FrameDecoder` begins at `framing.ts:53` and is not part of this slice.
