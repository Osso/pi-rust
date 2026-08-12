# Agent-core harness-error traceability

Pinned source: `packages/agent/src/harness/types.ts:137-181` at `532f7d12cd19235081bd4d6e425bde185652d86f`; public source-package exports are `packages/agent/src/index.ts:120-124`.

The focused Rust slice ports the typed error prerequisites only. Direct pinned differential proof uses the persistent utility oracle: 14 controlled constructor requests (`13000..=13013`) load `packages/agent/src/harness/types.ts` and compare normalized recursive values.

| TypeScript source | Observable behavior | Rust proof or gap |
|---|---|---|
| `types.ts:137-145` | Define the eight exact `FileErrorCode` literals | `FileErrorCode`, exhaustive focused literal assertions, explicit `as_str()` mapping, and direct fixtures `13000..=13007` |
| `types.ts:148-153` | `FileError` exposes `name`, `message`, `code`, and optional `path` | Public Rust fields, exact constructor/name assertions, Unicode/NUL/path focused proof, and direct normalized fields in fixtures `13000..=13007`; `None` represents source `path: undefined` |
| `types.ts:155-160` | Constructor order is `(code, message, path?, cause?)`; cause is conditionally attached | `FileError::new(code, message, path, cause)`, focused absent/present cause/path tests, and direct constructor branches in fixtures `13000..=13007` |
| `types.ts:163-169` | Define the six exact `ExecutionErrorCode` literals | `ExecutionErrorCode`, exhaustive focused literal assertions, explicit `as_str()` mapping, and direct fixtures `13008..=13013` |
| `types.ts:172-181` | `ExecutionError` exposes `name`, `message`, `code`, and optional cause | Public Rust fields, exact constructor/name assertions, focused cause/chain proof, and direct normalized fields in fixtures `13008..=13013` |
| `types.ts:30-38` | Normalize arbitrary thrown values through `toError(unknown)` before effectful adapters construct typed errors | Explicit exclusion; generic JavaScript normalization is not ported |
| `env/nodejs.ts:81-124,359-687` | Filesystem operations construct mapped `FileError` values | Consumer mapping only; filesystem implementation and backend code mapping remain later work |
| `env/nodejs.ts:38-48,368-501` | Execution operations construct mapped `ExecutionError` values | Consumer mapping only; timeout, spawn, callback, cancellation, and process execution remain later work |
| `session/jsonl/errors.ts:1-21` | Session storage maps `FileError` codes and wraps original errors | Consumer mapping only; session integration remains later work |
| `shell-output.ts:21-27,55-197` | Shell capture detects and propagates `ExecutionError` | Consumer mapping only; shell capture remains later work |

Rust uses source-shaped public fields and `Option<Box<dyn Error + 'static>>` for typed causes, as approved August 12, 2026. Error structs derive only `Debug`; code enums derive only `Debug`, `Clone`, `Copy`, `PartialEq`, and `Eq`. The persistent `harness_error` oracle operation constructs the real pinned classes and recursively normalizes only kind, name, message, code, path, and controlled `Error` causes. JavaScript descriptors, enumeration, serialization, stack text, prototype identity, arbitrary runtime values, and lone surrogates are outside the typed Rust boundary.
