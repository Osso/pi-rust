# Lossless Pyrun checkpoint

Frozen source: `4f9e6f8a4c0065d81b10e7177b28d5082f0bc30d`.

| Surface | Native coverage | Proof |
|---|---|---|
| `detached-bridge.ts` request/response DTOs, persistence and validation | Methods, command/protocol, request/tool/job/session IDs, DB/session paths and errors preserve UTF-16 units | `tests/lossless_pyrun.rs::persisted_bridge_request_retains_raw_method_ids_and_params`; pending coding-library compilation, not runtime evidence |
| `runner.ts` `PiRequest.method` ingress | Runner JSON retains raw method units | Pending real runner process proof |
| `index.ts` built-in dispatch | Scalar comparison selects known ASCII methods; unpaired methods reach unknown-method handling without modifying diagnostic text | Pending dispatcher proof |

`detached-runner.ts` manifest/activation paths, invocation argv, persisted tool IDs, request correlation and claim paths are native. Activation uses lossless JSON serialization rather than serde_json serialization of raw strings. `lossless_pyrun` adds real-file activation round-trip and invocation unit assertions; these remain unexecuted pending coding-library compilation. Detached evaluation now preserves native artifact-root derivation, script and bridge paths, recovery diagnostics, lifecycle errors, IDs and foreground methods. Provider tool-call correlation is explicitly validated as Unicode scalars only at dispatch. Existing fragmented-artifact/recovery unit tests use native APIs; runtime execution remains pending. Canonical runner metadata remains in migration. Persisted provider tool-call IDs require explicit rejection at the scalar dispatcher boundary (approved by parent); no lossy adapter is permitted. Real control-DB/lifecycle tests remain unexecuted until the coding library compiles; no replacement DB harness is used.
