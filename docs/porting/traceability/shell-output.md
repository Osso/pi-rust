# Agent-core shell-output traceability

Pinned source: `packages/agent/src/harness/utils/shell-output.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

The current slices map typed filesystem/execution error prerequisites, focused and pinned `FileKind`/`FileInfo` metadata proof, and exported `sanitizeBinaryOutput`. The metadata declaration and consumer mapping are maintained in [file-metadata traceability](file-metadata.md); `file_info.rs` provides focused value proof, `oracle/file-metadata-typecheck.mjs` proves the erased public types, and 10 runtime record/binary64 fixtures (`13014..=13023`) compare plain records through the persistent utility oracle. No runtime `FileInfo` constructor is claimed. The typed-error slice has focused and direct differential proof: 14 controlled `harness_error` fixtures (`13000..=13013`) invoke the real pinned classes through the public `packages/agent/src/index.ts` exports and persistent utility oracle. Sanitizer differential proof invokes the real pinned agent-core module through the same oracle. Eighteen valid-UTF-8 fixtures (`11000..=11017`) compare exact text, UTF-16 units, and code points; four source-only UTF-16 fixtures (`11018..=11021`) pin retained lone-surrogate behavior without constructing invalid Rust strings. Oracle framing uses a UTF-8 `StringDecoder` and LF-only boundaries; the whitespace fixture at `sanitize.rs:86` proves U+2028 and U+2029 remain payload data.

## Typed filesystem and execution errors

| TypeScript source | Observable behavior | Rust proof or gap |
|---|---|---|
| `types.ts:137-145` | Exact `FileErrorCode` literals | `FileErrorCode`, exhaustive focused assertions, and direct fixtures `13000..=13007` |
| `types.ts:148-160` | `FileError` name, message, code, optional path, constructor order, and typed cause | Public fields, focused constructor/path/cause/chain tests, and direct normalized fields in fixtures `13000..=13007`; `None` represents source `path: undefined` |
| `types.ts:163-169` | Exact `ExecutionErrorCode` literals | `ExecutionErrorCode`, exhaustive focused assertions, and direct fixtures `13008..=13013` |
| `types.ts:172-181` | `ExecutionError` name, message, code, constructor order, and typed cause | Public fields, focused constructor/cause/chain tests, and direct normalized fields in fixtures `13008..=13013` |
| `env/nodejs.ts:81-124,359-687` | Filesystem error production and backend code mapping | Consumer mapping only; filesystem implementation remains later work |
| `env/nodejs.ts:38-48,368-501` | Execution error production and backend branches | Consumer mapping only; process execution remains later work |
| `session/jsonl/errors.ts:1-21` | File-error code conversion and cause wrapping | Consumer mapping only; session integration remains later work |
| `shell-output.ts:21-27,55-197` | Execution-error detection and propagation | Consumer mapping only; shell capture remains later work |

Rust uses the approved `Option<Box<dyn Error + 'static>>` cause representation. The persistent oracle adds the public agent package index as its third source module and maps `harness_error` requests to recursive controlled `Error` causes; JavaScript descriptors, enumeration, serialization, stack text, prototype identity, arbitrary runtime values, and lone surrogates remain outside this typed boundary.

## Filesystem metadata values

| TypeScript source | Observable behavior | Rust proof or gap |
|---|---|---|
| `types.ts:133-134` | `FileKind` is the erased public union `"file" | "directory" | "symlink"` | `oracle/file-metadata-typecheck.mjs` proves exact union equality from the public `index.ts`; focused Rust tests assert all literals |
| `types.ts:213-224` | `FileInfo` is a mutable required-field record with `name`, `path`, `kind`, `size`, and `mtimeMs` | The generated strict fixture proves exact structure and assignments; `file_info.rs` proves the Rust public representation |
| `index.ts:125-126` | Both metadata types are type-only public exports | The generated fixture imports both from the absolute pinned public index; no runtime constructor is claimed |
| `types.ts:213-224` | `number` fields accept unrestricted binary64 values | `file_info.rs` and differential fixtures `13014..=13023` compare lowercase 16-digit `f64` bits for signed zero, subnormals, fractions, unsafe-integer boundaries, finite extremes, infinities, and NaN payload/sign patterns |

The differential `file_info` operation constructs a plain mutable JavaScript record because `FileInfo` and `FileKind` are erased types. It reconstructs each input with `DataView`/`BigUint64` and normalizes the record back to exact lowercase 16-digit binary64 strings; it does not execute a source constructor.

## Binary-output sanitization

| TypeScript source | Observable behavior | Rust proof |
|---|---|---|
| `shell-output.ts:30` | Export a pure string-to-string sanitizer | Public `sanitize_binary_output(&str) -> String`, focused ordinary/empty tests, and exact differential output comparison |
| `shell-output.ts:31` | Iterate with `Array.from`, preserving code-point order and supplementary pairs | Rust scalar iteration plus focused/direct supplementary, order, duplicate, UTF-16-unit, and code-point fixtures |
| `shell-output.ts:33-34` | Defensively reject an undefined code point | Typed-string exclusion: Rust iteration always yields a valid scalar; four direct UTF-16-unit fixtures separately pin the source's retained lone surrogates |
| `shell-output.ts:35` | Preserve tab, LF, and CR | Focused complete C0 range plus direct preserved-control and mixture fixtures |
| `shell-output.ts:36` | Remove every other value through U+001F | Focused generated range plus direct all-C0, boundary, mixture, and all-removed fixtures |
| `shell-output.ts:37` | Remove U+FFF9 through U+FFFB | Focused and direct U+FFF8..=U+FFFC boundary and mixture tests |
| `shell-output.ts:38-40` | Preserve every other code point and join without normalization | Focused and direct DEL/C1, whitespace/separator, format, BOM, bidi, private-use, unassigned, noncharacter, combining, variation, supplementary, ordering, duplicate, and all-removed fixtures |
| `shell-output.ts:121` | Captured stdout/stderr is sanitized, then CR is removed before accounting | Consumer mapping only; capture state and CR removal remain a later slice |

Typed-string exclusion: `Array.from` retains lone surrogates in the source implementation. Rust `str` cannot represent them, so the four source-only UTF-16 fixtures pin that behavior without constructing invalid Rust strings.
