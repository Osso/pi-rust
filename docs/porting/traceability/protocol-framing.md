# Protocol-framing traceability

Pinned source: `packages/protocol/src/framing.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Differential proof: `differential_framing.rs::framing_matches_the_pinned_typescript_oracle` executes the real source for stateless framing operations and ordered decoder sequences. Focused Rust tests separately prove the approved allocation-failure behavior.

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
| `framing.ts:58-67` | Retain header, payload-block, expected-length, and terminal-state fields | Focused fragmented/block/terminal tests; all decoder sequence fixtures |
| `framing.ts:69-71` | Construct open decoder with default or configured maximum | Focused default/exact-maximum tests; matching decoder fixtures |
| `framing.ts:74-76` | Reject pushes after ended/failed before reading input | Focused terminal tests; differential ended/failed sequences |
| `framing.ts:78-86` | Accumulate a four-byte header across arbitrary chunks | Focused fragmented/every-split tests; byte-at-a-time and every-split sequences |
| `framing.ts:88-93` | Decode length, reset header, and fail immediately above limit | Focused limit tests; differential oversized/default-limit/same-push sequences |
| `framing.ts:94-102` | Emit empty frames or initialize nonempty payload state | Focused and differential fragmented/coalesced/empty cases |
| `framing.ts:105-123` | Allocate/reuse 64 KiB blocks and copy chunk bytes | Focused block/ownership tests; 65,536/65,537/70,000-byte and mutation sequences |
| `framing.ts:124-135` | Return one block directly or merge multiple blocks | Focused and differential block-boundary cases |
| `framing.ts:136-143` | Reset completed payload state and return frames in wire order | Focused coalesced/later-activity tests; differential fragmented/coalesced sequences |
| `framing.ts:147-153` | Keep terminal states, reject truncation, or end cleanly | Focused empty/truncation/terminal tests; matching ordered sequences |
| `framing.ts:155-164` | Enter failed state and clear retained partial data | Focused oversize/truncation/post-failure tests; differential repeated failed operations |

Approved Rust-only behavior: every decoder-owned `Vec` growth in `push` uses fallible reservation. Allocation failure returns `FrameDecodeError::Allocation`, clears partial state, and makes the decoder terminally failed.
