# Auth storage

Baseline: frozen `4f9e6f8a4`, `packages/coding-agent/src/core/auth-storage.ts`.

Auth sync and async lockers use 30,000ms stale policy; async heartbeat updates every 15,000ms. Sync acquisition retries ten attempts, 20ms between ELOCKED failures. Async acquisition retries ten times with factor two, randomized 100ms minimum, 10,000ms maximum. Auth policy must not change global lock defaults.

Async backend rejects a compromised lock before callback, after callback and after write. Unlock errors are ignored on async completion; synchronous release errors propagate. Cancellation-bearing Rust operations additionally reject cancellation before writing; successful OAuth rotation must be rescued by the source-equivalent refresh path.

Successful OAuth refresh spends the old credential. The source attempts locked persistence, then fresh-storage rescue preserving unrelated providers. Public refresh/get_api_key remains unimplemented in the offered checkpoint. If all persistence and reload attempts fail, frozen TS returns the rotated credential while disk retains the spent one; user classification is pending. No rescue-failure contract or same-provider newer-token arbitration is approved.

Only synthetic disposable credentials may be used in parity proofs. Source public API parity and obsolete native consumer replacement remain separate tracked work until proved.
