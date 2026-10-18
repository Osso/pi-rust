# Agent-core shell-output traceability

Pinned source: `packages/agent/src/harness/utils/shell-output.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

The current slices map typed filesystem/execution error prerequisites, focused and pinned `FileKind`/`FileInfo` metadata proof, exported `sanitizeBinaryOutput`, and the full `executeShellWithCapture` pipeline. The metadata declaration and consumer mapping are maintained in [file-metadata traceability](file-metadata.md); `file_info.rs` provides focused value proof, `oracle/public-index-typecheck.mjs` proves the erased public types, and 10 runtime record/binary64 fixtures (`13014..=13023`) compare plain records through the persistent utility oracle. No runtime `FileInfo` constructor is claimed. The typed-error slice has focused and direct differential proof: 14 controlled `harness_error` fixtures (`13000..=13013`) invoke the real pinned classes through the public `packages/agent/src/index.ts` exports and persistent utility oracle. Sanitizer differential proof invokes the real pinned agent-core module through the same oracle. Eighteen valid-UTF-8 fixtures (`11000..=11017`) compare exact text, UTF-16 units, and code points; four source-only UTF-16 fixtures (`11018..=11021`) pin retained lone-surrogate behavior without constructing invalid Rust strings. Oracle framing uses a UTF-8 `StringDecoder` and LF-only boundaries; the whitespace fixture at `sanitize.rs:86` proves U+2028 and U+2029 remain payload data. Capture proof lives in `tests/shell_output_capture.rs` (6 tests) and the bash differential fixtures.

## Typed filesystem and execution errors

| TypeScript source | Observable behavior | Rust proof or gap |
|---|---|---|
| `types.ts:137-145` | Exact `FileErrorCode` literals | `FileErrorCode`, exhaustive focused assertions, and direct fixtures `13000..=13007` |
| `types.ts:148-160` | `FileError` name, message, code, optional path, constructor order, and typed cause | Public fields, focused constructor/path/cause/chain tests, and direct normalized fields in fixtures `13000..=13007`; `None` represents source `path: undefined` |
| `types.ts:163-169` | Exact `ExecutionErrorCode` literals | `ExecutionErrorCode`, exhaustive focused assertions, and direct fixtures `13008..=13013` |
| `types.ts:172-181` | `ExecutionError` name, message, code, constructor order, and typed cause | Public fields, focused constructor/cause/chain tests, and direct normalized fields in fixtures `13008..=13013` |
| `env/nodejs.ts:81-124,359-687` | Filesystem error production and backend code mapping | `file_error`/`rename_error` in `env/nodejs.rs`; see [node execution env](harness-nodejs-env.md) |
| `env/nodejs.ts:38-48,368-501` | Execution error production and backend branches | `exec_command` in `env/nodejs.rs`; see [node execution env](harness-nodejs-env.md) |
| `session/jsonl/errors.ts:1-21` | File-error code conversion and cause wrapping | Consumer mapping only; session integration remains later work |
| `shell-output.ts:21-27,55-197` | Execution-error detection and propagation | `execute_shell_with_capture`/`to_execution_error`; capture tests and bash differential fixtures |

Rust uses `Option<Box<dyn Error + Send + Sync + 'static>>` for typed causes so env futures and `AgentTool::execute` stay `Send`; this widens the earlier `Box<dyn Error + 'static>` representation without changing normalized fields. The persistent oracle adds the public agent package index as its third source module and maps `harness_error` requests to recursive controlled `Error` causes; JavaScript descriptors, enumeration, serialization, stack text, prototype identity, arbitrary runtime values, and lone surrogates remain outside this typed boundary.

## Filesystem metadata values

| TypeScript source | Observable behavior | Rust proof or gap |
|---|---|---|
| `types.ts:133-134` | `FileKind` is the erased public union `"file" | "directory" | "symlink"` | `oracle/public-index-typecheck.mjs` proves exact union equality from the public `index.ts`; focused Rust tests assert all literals |
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
| `shell-output.ts:121` | Captured stdout/stderr is sanitized, then CR is removed before accounting | `account_chunk`; `sanitizes_chunks_and_removes_carriage_returns` |

Typed-string exclusion: `Array.from` retains lone surrogates in the source implementation. Rust `str` cannot represent them, so the four source-only UTF-16 fixtures pin that behavior without constructing invalid Rust strings.

## Shell capture

| TypeScript source | Observable behavior | Rust proof |
|---|---|---|
| `shell-output.ts:4-9` | `ShellCaptureProgress` output/truncation/fullOutputPath/lastLineBytes | `ShellCaptureProgress`; capture and progress tests |
| `shell-output.ts:11-17` | Capture options: shell options plus chunk callback and `returnExecutionErrors` | `ShellCaptureOptions`; error/timeout tests |
| `shell-output.ts:19-27` | Capture result extends progress with exitCode/cancelled/truncated/executionError; unknown errors wrap with cause message | `ShellCaptureResult`/`to_execution_error`; cancelled/execution-error tests |
| `shell-output.ts:29-40` | Sanitizer | See sanitizer table above |
| `shell-output.ts:42-48` | Trim the retained tail at a valid UTF-8 boundary | `trim_to_last_utf8_bytes`; oversized-chunk test |
| `shell-output.ts:55-103` | Tail buffer, byte/line counters, open-line state, full-output temp file creation and append chain | `CaptureState`/`write_full_output`; large and oversized-chunk tests |
| `shell-output.ts:105-127` | Progress derivation: tail truncation plus total counters and truncatedBy fallback | `progress_snapshot`/`capture_truncation`; progress and truncation tests |
| `shell-output.ts:129-163` | Chunk accounting, CR removal, full-output request, tail trim, user callback | `account_chunk`; truncation and progress tests |
| `shell-output.ts:165-197` | Exec, stop accepting output, post-exec file request, write-chain errors, cancelled/timeout/execution-error result mapping | `execute_shell_with_capture`; capture tests and bash differential fixtures |

Rust boundary: the user chunk callback receives the progress snapshot computed after accounting instead of a lazy `getProgress` closure. Full-output writes run in a task that owns an `Arc<dyn ExecutionEnv>` clone, preserving source ordering through an unbounded channel.
