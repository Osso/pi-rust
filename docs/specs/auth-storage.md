# Auth storage

Baseline: frozen `4f9e6f8a4`, `packages/coding-agent/src/core/auth-storage.ts`.

Auth sync and async lockers use 30,000ms stale policy; async heartbeat updates every 15,000ms. Sync acquisition retries ten attempts, 20ms between ELOCKED failures. Async acquisition retries ten times with factor two, randomized 100ms minimum, 10,000ms maximum. Auth policy must not change global lock defaults.

Async backend rejects a compromised lock before callback, after callback and after write. Unlock errors are ignored on async completion; synchronous release errors propagate. Cancellation-bearing Rust operations additionally reject cancellation before writing; successful OAuth rotation must be rescued by the source-equivalent refresh path.

Successful OAuth refresh spends the old credential. The source attempts locked persistence, then fresh-storage rescue preserving unrelated providers. Public refresh/get_api_key follows the frozen module plus approved 2026-10-21 delta `0c9f0390` (locked checkout `26c8050f`). If locked write, rescue set, and reload all fail, memory retains the rotated credential but get_api_key returns None, or throws the authentication wrapper with the persistence error as cause when requested. Only successful reload can prove another instance persisted a usable credential. No same-provider newer-token arbitration is approved.

Only synthetic disposable credentials may be used in parity proofs. Source public API parity and obsolete native consumer replacement remain separate tracked work until proved.

Public raw credential values, provider IDs, getters/list results, and typed OAuth access/refresh/extras use `JsValue`/`JsString` with lossless parse/stringify. There is no parallel raw credential copy. Required OAuth strings remain UTF-16 until request-provider handoff. Legacy serde adapters reject unrepresentable Unicode instead of replacing it; they are not the public persistence path.

Outbound OAuth boundaries follow the invoked Node API, not a storage-wide UTF-8 conversion: JSON body serialization escapes isolated surrogates; URLSearchParams form values undergo USVString conversion only when constructing the form request; HTTP header values reject invalid code units. These conversions never alter the canonical stored token. Small Node differentials cover JSON/form output and header error name/message. The legacy callback error carrier has no Node `code` property; direct-provider error-code shape remains a gap.
