# Fallible crypto randomness

## Requirements

- OAuth `RandomSource::fill` and `uuid`, PKCE generation, and OAuth state/UUID helpers return `Result` with the original `CallbackError`. No panic or random fallback on entropy failure.
- OS entropy failures use Node's empty-OpenSSL-queue error: `Error`, message `Deriving bits failed`, code `ERR_CRYPTO_OPERATION_FAILED`. [Pinned Node provenance](../porting/node-crypto-random-source-lock.json). Parent approved this mapping on 2026-10-25; native getrandom has no OpenSSL error queue.
- Generate the same 32-byte PKCE verifier and SHA-256 challenge as frozen `utils/oauth/pkce.ts:21–33`. Consume entropy once and stop before later OAuth side effects on failure.
- Propagate at frozen throws and convert only at frozen catches. Keep raw rejection values and UTF-16 error text.
- Keep `challenge_for` private: frozen exports only `generatePKCE`. Test the public generator against actual-source deterministic crypto inputs instead of an obsolete public helper.

## Checkpoint boundaries

1. OAuth randomness and PKCE become fallible; Anthropic/Codex login and custom provider examples propagate before callback setup. System `utils/random_uuid.rs` remains byte-identical at this first checkpoint.
2. The approved separate UUID migration removes the existing system-UUID panic and widens required UI/settings/bridge/error carriers mechanically. This remains open until that checkpoint; no overall entropy-path completion claim from step1.

## UUID consumer carriers

System UUID returns `Result<String, NativeNodeError>` from the same entropy authority; injected OAuth randomness remains `CallbackError`. Existing native-error wrappers retain its identity. UI widget/title/editor setters, paste delegation and dialog failures use `CallbackError`; settings analytics and ArchitectObserver construction become fallible. Pyrun bridge and detached cleanup carry the error until an actual frozen catch converts it. Harness/filesystem wrappers keep the original native error as their source.

Frozen clipboard handling catches UUID errors in interactive-mode.ts:3835–3854. PowerShell image temp-name generation is outside clipboard-image.ts's try and must propagate. Detached cleanup builds quarantine UUID outside its filesystem try, so the crypto failure bypasses directory recovery and reaches its outer catch. No error is routed to the uncaught authority merely because an old Rust interface was infallible.

## Proof

`pkce_repin.rs` compares complete source/native deterministic pairs and source rejection values; the private crypto authority test forces an OS-source error. Existing OAuth/example flows retain their success transcripts. No live OAuth or provider endpoints, dependencies, async-scheduler emulation or uncaught routing are added.
