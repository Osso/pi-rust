# auth/oauth/pkce traceability

Pinned source: `packages/ai/src/auth/oauth/pkce.ts` at commit `532f7d12cd19235081bd4d6e425bde185652d86f`.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `base64urlEncode(bytes)` | Standard Base64 with `+`→`-`, `/`→`_`, padding removed. | Private `pkce::base64url_encode`; exercised through verifier/challenge generation and known SHA-256 vectors. |
| `generatePKCE()` | 32 random bytes → 43-char base64url verifier; challenge = base64url(SHA-256(verifier)). | `generate_pkce` using `getrandom` and `sha2`; `generates_a_base64url_verifier_and_sha256_challenge`. |
| `challenge_for(verifier)` | Same SHA-256/base64url transformation. | Public `challenge_for`; `challenges_match_known_sha256_vectors`. |
| Differential oracle | Eight TS-generated verifier/challenge pairs plus charset/length checks. | `tests/differential_oauth.rs` recomputes each challenge in Rust and compares; Rust generation is self-consistent. |

## Exclusions

- The TypeScript fallback from Web Crypto to `Math.random` applies only to UUID generation, not PKCE; Rust panics if the OS random source fails rather than producing a predictable verifier.
- Verifier randomness itself is not compared (non-deterministic); only the derived challenge and format are.
