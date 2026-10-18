# Harness result traceability

Pinned source: `packages/agent/src/harness/result.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

No runtime artifact is added: the source module is a TypeScript-only encoding of idioms that Rust models directly.
There is no `result.test.ts` in the pinned package.

| TypeScript source | Observable contract | Rust mapping |
|---|---|---|
| `result.ts:1` | `Result<TValue, TError>` is `{ok:true,value}` or `{ok:false,error}` | `std::result::Result<T, E>` |
| `result.ts:4-6` | `Result.ok(value)` constructs success | `Ok(value)` |
| `result.ts:7-9` | `Result.err(error)` constructs failure | `Err(error)` |
| `result.ts:10-12` | `Result.isOk` narrows to success | `Result::is_ok()` / `matches!` |
| `result.ts:13-15` | `Result.isErr` narrows to failure | `Result::is_err()` |
| `result.ts:17-21` | `TaggedErrorValue<Tag>` carries `_tag`, `Error` identity, and `toJSON()` including `_tag`, `message`, and every own property | Typed error structs/enums: `SessionError` (`code`, `cause`, `message`), `FileError`, `ExecutionError`, `CompactionError`, `BranchSummaryError`, `RecordLogCorruption` (`reason`, `message`), `JsonlDecodeError` (`kind`). Each exposes a literal accessor and `Display`/`Error`. |
| `result.ts:23-31` | `TaggedErrorFactory` constructs, assigns props, sets `name = tag`, and narrows with `is` | Per-module error constructors plus `reason()`/`code()`/`kind()` and enum-typed fields; no dynamic property assignment exists in the typed boundary. |
| `result.ts:33-46` | `TaggedError(tag)` builds the class at runtime | Closed Rust enums/structs; `match` replaces runtime tag dispatch. |
| `result.ts:48-54` | `matchError(error, matchers)` dispatches on `_tag` and calls the matcher | `match`/`if let` on the typed error enum. |

## Exclusions

- Dynamic construction of error classes from arbitrary string tags, `Object.assign` property copying, and `toJSON` key enumeration are JavaScript runtime capabilities with no typed Rust equivalent. Every source error tag used by the ported packages has a concrete Rust type instead (see `session-types.md`, `harness-errors.md`, `session-jsonl-errors.md`).
- No dependency or helper module is added.
