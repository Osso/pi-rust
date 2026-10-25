# utils/oauth/pkce traceability

Frozen: `packages/ai/src/utils/oauth/pkce.ts`, common pin `4f9e6f8a4`. [Fallible randomness contract](../../specs/crypto-random-failures.md).

| Source | Native behavior | Proof |
|---|---|---|
| `base64urlEncode` :9–15 | Private unpadded URL-safe encoder; both verifier and hash | `pkce_repin`: complete deterministic pairs for zero,255,incrementing and mixed bytes; random format test |
| `generatePKCE` :21–25 | Request32bytes once, propagate rejection before hashing | Source crypto call record; native single fill count and exact raw/Error rejection |
| SHA-256/TextEncoder :28–31 | SHA-256 of ASCII verifier bytes | Actual-source full verifier/challenge equality and observed digest bytes/algorithm |
| Return :33 | `Pkce` owns verifier/challenge | Complete pair comparison, no normalization |

`generate_pkce_with` is the native dependency seam used by built-in OAuth flows and the frozen custom-Anthropic/GitLab consumers; it is not an alternate generator implementation. `challenge_for` is private like frozen's local hash calculation. Its old public-helper vector test and aggregate eight-source-random/recomputed-helper comparisons are retired in favor of the public generator's deterministic actual-source pair comparisons. The native random-format test remains.

Source-probe `pkce_repin::frozen_crypto_failures_reject_exact_values_without_fallback` proves both getRandomValues and digest rejection, including arbitrary raw UTF16. Native SHA-256 has no configurable WebCrypto backend or fallible digest operation; it computes the fixed algorithm directly. Native entropy failure returns the exact approved Node empty-queue error through [one authority](../../specs/crypto-random-failures.md). No insecure fallback, panic conversion, or fabricated OpenSSL diagnostics.

First prerequisite widens RandomSource fill/uuid, PKCE/state/UUID helpers and immediate OAuth/example consumers. Frozen Anthropic PKCE generation precedes server/browser setup; Codex PKCE and state precede URL/browser/server work. The original rejection propagates through AuthError or HandlerError, not a caught string. `utils/random_uuid.rs` and all of its wider consumers remain unchanged until the second approved checkpoint.

The former oracle imported retired `auth/oauth` paths. It now uses `utils/oauth` only; no fallback paths. Whole OAuth flows remain separate rows, not closed by PKCE helper proof. Current command/revision/results live in `/tmp/pi-rust-repin-delta-ai/q15-proof-ledger.md`.

## First prerequisite checkpoint

At0458474dc: current PKCE3 +six actual-source/native OAuth failure cases1 GREEN (`q15f-oauth-entropy-current-fixtures.log`); native OS-failure authority1, PKCE random-format1, Anthropic6, Codex8, existing page/flow differential targets2 and custom-Anthropic/GitLab19 active tests GREEN in bounded runs. The six failure cases cross Anthropic first-fill, Codex PKCE first-fill and Codex state second-fill with branded Error and raw UTF16; exact rejection values match frozen, no network request or auth notification occurs, and the Codex method prompt remains observable. Private GitLab conversion child is invoked by its active process test.

Initial flow fixture incorrectly replaced randomBytes per request even though frozen caches the first imported callback. Fixed to one stable function with per-request state. A no-fetch guard prevents any source fixture from attempting provider traffic. Initial missing RandomSource implementation in the Copilot fixture was a compilation failure, fixed before the final0/0 gate. None of these setup failures are source discrepancies.

Workspace/all-target JSON check0errors/0warnings; all-feature/all-target warning Clippy0errors/1664existing warnings/0changed-line and0new signatures versus pre-slice (`q15f-oauth-entropy-check-2.jsonl`, `clippy-1.jsonl`, `clippy-owned/new.json`). Protected strict frozen-builder+selected-source/fixture TS passes (`q15f-oauth-entropy-ts-typecheck-2.log`); Biome check passed after import organization. Manual changed-line Rust/TS readability: shared entropy authority, pure PKCE transform, immediate Result propagation, explicit fixture state/effects; no blocker or new suppression/dependency. Metrics executable unavailable.

Separate existing failure found while compiling the newly fallible Copilot test double: `differential_github_copilot_oauth::refresh_matches_ts` has native availableModelIds `["gpt-5.4"]` versus frozen `[]`; to_auth passes. Production github_copilot.rs is byte-identical to pre-slice, and only the test-double signature/Ok wrapping changed. This remains queued whole-Copilot work, not a randomness fix or passing package-wide claim. `q15f-oauth-entropy-final-fixtures.log` records that failure; current PKCE/OAuth failure targets were then run separately because that command stopped before reaching them.
