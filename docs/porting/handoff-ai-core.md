# AI core lane handoff — 2026-10-25

**Successor completion:** all seven remaining rows are closed; the original six remain done. Current audit, fixes, exact code revision, tests, warning classification and inventory are in [AI core successor checkpoint](ai-core-successor-proof.md). The remainder below is the historical predecessor handoff, not current pending work.

Worktree `/home/osso/.worktrees/pi-rust-repin-delta-ai`, branch `repin/delta-ai`. Stopped on parent instruction (agent cap), not task completion. All current code compiles: `cargo check -j3 --workspace --all-targets --keep-going`, exit0, `/tmp/ai3-types-workspace-check.log`. No uncompiled patch or background job.

## Closed rows (6 of13 assigned)

- compat, images-models, Faux, retry, validation, device-code. Their module traces and coverage rows record actual proof and approved gaps.
- Commits: e673bf2c6 retry removal; b670c5093 compat ambient marker/root forwards;857e9fbbd image rejection/refresh;960a5c763 real-tool validation; c35ff77ca Faux native text/fractional chunks;180bf9a0b device polling Result.
- Parent de46a2789 merged at829ceab58; only conflict was coverage status totals, recomputed. Parent latest1ebc46773 was searched explicitly for sampling_params; every occurrence is covered by the current retirement. It is not yet merged.

## Current compiling slice

Source-absent samplingParams typed fields are removed from Model/ImagesModel/StreamOptions. Unknown/historical model keys use existing ModelProperties. ImagesModel now uses that same lossless unknown/order authority and native display name; ImagesOptions.metadata uses JsObject. Event decoding now reads through JsValue instead of serde internally-tagged buffering, which rejected raw UTF-16 in valid stream events. Parent approved retirement and mechanical fallout, including simple_options constructor and test fixture changes; no helper algorithm changed.

Proof: compile RED for image metadata; behavioral RED for historical sampling keys/image display fields; complete frozen type witness exposed event UTF-16 rejection. GREEN23types+1publicOAuth tests (`/tmp/ai3-types-green-final.log`),13image tests (`/tmp/ai3-types-suite_modes.log`),2simple-options source differentials (`/tmp/ai3-sampling-options-green.log`). Strict frozen-builder witness/fixture typecheck PASS (`/tmp/ai3-contracts-typecheck.log`). Formatting after these proofs is semantic-neutral. No scoped clippy/readability final gate has run in this successor; do not claim whole-lane completion.

## Remaining rows (7)

- `ai/src/types.ts`: whole source read; current carrier repairs proven, but final trace cleanup/public-surface audit and scoped clippy still pending. Historical trace contains retired deferred/constrained names, clearly historical; replace stale mapping rather than treating it as current proof.
- `ai/src/oauth.ts`, `utils/oauth/index.ts`, `utils/oauth/types.ts`: audit registry mutation/restore/order/errors, direct exports and builtin callback bridge. Root barrel is still one reexport. Existing registry tests are standalone targets. Keep direct provider implementation proofs separate.
- `utils/oauth/anthropic.ts`, `github-copilot.ts`, `openai-codex.ts`: whole-flow audit pending. Existing loopback/fake-fetch differentials and native suites are available. Codex credentialsFromToken/OAuthCredential key order were merged from agent266 and must not be reimplemented.

## Next work

- Run scoped warning-mode clippy for changed AI lib/test targets and inspect changed-line warnings; manual Rust/TS readability. Required final lane proof was not run because of the stop.
- Merge the current parent before further implementation. Preserve6closed rows and recompute coverage counts; run source-inventory before checkpoint.
- Audit OAuth modules against immutable4f9e6f8a4. Do not make live OAuth/provider calls. Native Copilot/Codex polling closures now return Result but still flatten their transport failures into Failed.message; reproduce ordinary callback/fetch rejection identity before changing those flow-owned catch policies.
- Image source discrepancy is deliberate: no-auth generateImages returns an unawaited provider promise and can reject despite its comment. Codex-image-generation does not consume ImagesModels; it uses compat.streamSimple/hosted image tools. Parent asked for user-visible Codex proof; no new consumer test was added because there is no call path. Authenticated image rejection remains caught; an explicit added differential for that unchanged branch is useful remaining proof.

## Parent classifications

Synthetic nested Unsafe integer versus Integer metadata mismatch: inspect real tools first. Actual frozen/native history-tool Integer coercion matches6inputs; tracked Unsafe appears only through StringEnum, no real Unsafe integer node found. Parent permits a one-line gap instead of adding a per-node carrier. Randomness/ID values, cold/dropped futures, browser/readiness and runtime/timer oddities are approved gaps. Keep ordinary logic/chunk arithmetic/usage exact.
