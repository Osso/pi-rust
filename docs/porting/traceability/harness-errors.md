# Agent-core harness-error traceability

Pinned source: `packages/agent/src/harness/types.ts:137-211` at `532f7d12cd19235081bd4d6e425bde185652d86f`; public source-package exports are `packages/agent/src/index.ts:114-124`.

The focused Rust slice ports typed error prerequisites only. Filesystem/execution and compaction/branch-summary errors now have direct pinned proof; effectful producers remain later slices.

| TypeScript source | Observable behavior | Rust proof or gap |
|---|---|---|
| `index.ts:120-124` | Publicly re-export both error classes and code types | Focused Rust imports all four public harness symbols; the oracle loads `index.ts` and constructs both runtime classes through those exports; TypeScript code types are compile-time-only |
| `types.ts:137-145` | Define the eight exact `FileErrorCode` literals | `FileErrorCode`, exhaustive focused literal assertions, explicit `as_str()` mapping, and direct fixtures `13000..=13007` |
| `types.ts:148-153` | Declare the `FileError` subclass with public `code` and optional `path` | Public Rust type and fields plus focused public-import, code, path, and mutation assertions |
| `types.ts:154-160` | Preserve constructor order `(code, message, path?, cause?)`, inherited message, explicit name, code, path, and conditional cause | `FileError::new(code, message, path, cause)`, focused absent/present cause/path tests, and direct normalized fixtures `13000..=13007`; `None` represents source `path: undefined` |
| `types.ts:163-169` | Define the six exact `ExecutionErrorCode` literals | `ExecutionErrorCode`, exhaustive focused literal assertions, explicit `as_str()` mapping, and direct fixtures `13008..=13013` |
| `types.ts:172-175` | Declare the `ExecutionError` subclass with public `code` | Public Rust type and field plus focused public-import, code, and mutation assertions |
| `types.ts:176-181` | Preserve constructor order `(code, message, cause?)`, inherited message, explicit name, code, and conditional cause | `ExecutionError::new(code, message, cause)`, focused absent/present cause and nested-chain tests, and direct normalized fixtures `13008..=13013` |
| `types.ts:183-184` | Define the distinct two-literal `CompactionErrorCode` union | `CompactionErrorCode`, exhaustive focused literal assertions, exact `as_str()` mapping, and direct fixtures `13036..=13039, 13044` |
| `types.ts:186-196` | Preserve `CompactionError` name, public code/message fields, constructor order, and optional `Error` cause | `CompactionError::new(code, message, cause)`, focused absent/present cause, public mutation, `Display`, `Error::source()`, and direct normalized fixtures `13036..=13039, 13044` |
| `types.ts:198-199` | Define the distinct two-literal `BranchSummaryErrorCode` union | `BranchSummaryErrorCode`, exhaustive focused literal assertions, exact `as_str()` mapping, and direct fixtures `13040..=13043, 13045` |
| `types.ts:201-211` | Preserve `BranchSummaryError` name, public code/message fields, constructor order, and optional `Error` cause | `BranchSummaryError::new(code, message, cause)`, focused absent/present cause, public mutation, `Display`, `Error::source()`, and direct normalized fixtures `13040..=13043, 13045` |
| `types.ts:30-38` | Normalize arbitrary thrown values through `toError(unknown)` before effectful adapters construct typed errors | Explicit exclusion; generic JavaScript normalization is not ported |
| `env/nodejs.ts:81-124,359-687` | Filesystem operations construct mapped `FileError` values | Consumer mapping only; filesystem implementation and backend code mapping remain later work |
| `env/nodejs.ts:38-48,368-501` | Execution operations construct mapped `ExecutionError` values | Consumer mapping only; timeout, spawn, callback, cancellation, and process execution remain later work |
| `session/jsonl/errors.ts:1-21` | Session storage maps `FileError` codes and wraps original errors | Consumer mapping only; session integration remains later work |
| `shell-output.ts:21-27,55-197` | Shell capture detects and propagates `ExecutionError` | Consumer mapping only; shell capture remains later work |
| `types.ts:183-211` | Compaction and branch-summary classes expose distinct codes, exact names/messages, public mutation, and typed causes | `crates/pi-agent-core/tests/compaction_errors.rs` covers the focused Rust contract; `crates/pi-agent-core/tests/differential_truncate/errors.rs` compares 10 direct fixtures (`13036..=13045`) including ordinary and cross-class nested causes |
| `index.ts:114-119` | Publicly re-export compaction and branch-summary classes and code types | `oracle/truncate-oracle.ts` imports runtime constructors from `packages/agent/src/index.ts`; the same 10 fixtures prove the public-package boundary |

Rust uses source-shaped public fields and `Option<Box<dyn Error + 'static>>` for typed causes, as approved August 12, 2026. Error structs derive only `Debug`; code enums derive only `Debug`, `Clone`, `Copy`, `PartialEq`, and `Eq`. The closed recursive `harness_error` operation compares 24 harness-error fixtures: filesystem/execution `13000..=13013` and compaction/branch-summary `13036..=13045`; persistent metadata/resource requests occupy `13014..=13035`. Consumer mappings remain gaps: later adapters must prove source-compatible backend branches and messages without claiming those algorithms here. JavaScript descriptors, enumeration, serialization, stack text, prototype identity, arbitrary runtime values, and lone surrogates are outside the typed Rust boundary.
