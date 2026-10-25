# Auth storage

Baseline: frozen `4f9e6f8a4`, `packages/coding-agent/src/core/auth-storage.ts`.

Each AuthStorage instance owns an independent cached snapshot, loaded on construction and refreshed only by explicit reload or the source's mutation/refresh paths. Raw get/getAll/list do not auto-reload, resolve env/command references, or validate credential key types. No separate read-only validation/coalescing API is part of the frozen contract.

Auth sync and async lockers use 30,000ms stale policy; async heartbeat updates every 15,000ms. Sync acquisition retries ten attempts, 20ms between ELOCKED failures. Async acquisition retries ten times with factor two, randomized 100ms minimum, 10,000ms maximum. Auth policy must not change global lock defaults.

Async backend rejects a compromised lock before callback, after callback and after write. Unlock errors are ignored on async completion; synchronous release errors propagate. Successful OAuth rotation must be rescued by the source-equivalent refresh path.

Successful OAuth refresh spends the old credential. The source attempts locked persistence, then fresh-storage rescue preserving unrelated providers. Public refresh/get_api_key follows the frozen module plus approved 2026-10-21 delta `0c9f0390` (locked checkout `26c8050f`). If locked write, rescue set, and reload all fail, memory retains the rotated credential but get_api_key returns None, or throws the authentication wrapper with the persistence error as cause when requested. Only successful reload can prove another instance persisted a usable credential. No same-provider newer-token arbitration is approved.

In-memory async callbacks are not serialized: overlapping expired refreshes both read the current A credential before either callback persists B, matching frozen InMemoryAuthStorageBackend. File-backed callbacks retain their lock policy.

Caught non-Error refresh failures are recorded as `new Error(String(value))`; native Error identity/properties survive. String conversion can itself throw before reload or the optional authentication wrapper. Wrapper reasons use Error.message or String(value), preserving exact UTF-16.

Only synthetic disposable credentials may be used in parity proofs. Source public API parity and obsolete native consumer replacement remain separate tracked work until proved.

Persisted API-key credentials resolve their `key` through the frozen config parser. Missing/null keys and non-string JSON values throw the parser's exact TypeError message and constructor identity; they never become absent credentials. Raw getters remain unvalidated.

Public raw credential values, provider IDs, getters/list results, and typed OAuth access/refresh/extras use `JsValue`/`JsString` with lossless parse/stringify. There is no parallel raw credential copy. Public get_api_key returns JsString, including isolated surrogates from provider.get_api_key, without request encoding or validation. Required OAuth strings remain UTF-16 until the actual HTTP header/body boundary; JSON/form/header conversions belong there, not in public storage lookup. Old-only resolving CredentialStore and one-off reader adapters are not part of this API.

Outbound OAuth boundaries follow the invoked Node API, not a storage-wide UTF-8 conversion: JSON body serialization escapes isolated surrogates; URLSearchParams form values undergo USVString conversion only when constructing the form request; HTTP header values reject invalid code units. These conversions never alter the canonical stored token. Small Node differentials cover JSON/form output and header TypeError name/message plus code ERR_INVALID_CHAR, carried by CallbackError.code.

OAuth credential persistence preserves provider construction order, not a fixed token-field order. Parsed typed credentials retain original known/extra key positions. AuthStorage login/refresh emits the literal `{ type: "oauth", ...credentials }` spread order: type first, then provider fields in their original order. Codex token construction uses access, refresh, expires, accountId, then optional email. Nonempty direct id-token email wins over nonempty profile.email; absent/empty/undecodable id tokens add no email. JWT payloads retain frozen atob byte-string decoding.
