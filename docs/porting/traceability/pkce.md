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
