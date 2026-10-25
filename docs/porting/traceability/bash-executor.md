# coding-agent bash-executor traceability

Frozen `packages/coding-agent/src/core/bash-executor.ts` at `4f9e6f8a4`; whole module. Native `crates/pi-coding-agent-tools/src/core/bash_executor.rs`. [Contract/checkpoint](../../specs/bash-tool.md), [Bash integration](bash.md). Current-pin acceptance replaces the old `532f7d12` map; historical logs remain in the lane ledger.

Accumulator prerequisite follow-up replaces raw error-reporting threads with the shared claimable native-error slot and a task that first yields on the captured owning Tokio runtime. Retained callbacks reuse that handle even outside its entered scope; no queue or framework. Original native error identity and unhandled classification remain unchanged. Focused spill19 revalidation GREEN in `accumulator-deferred-retained.log`; unchanged byte/error assertions retained.

| Exact source symbol/branch | Native representation | Current proof |
|---|---|---|
| `BashExecutorOptions.onChunk`, `signal` | Optional fallible native chunk callback and shared reason-bearing signal | `differential_bash_hook_chunk_errors`: ASCII/D800 callback rejection, abort during callback; executor6 covers absent callback; `differential_bash_executor_stream`10 checks optional operation fields |
| `BashResult.output/exitCode/cancelled/truncated/fullOutputPath` | Native text/path, optional binary64 exit, booleans | Direct source numeric10 preserves NaN/infinities/-0 before JSON projection; result/RPC/session bytes audited in [Bash trace](bash.md#binary64-exit-code-byte-audit) |
| `executeBashWithOperations` forwards command/cwd/signal only | Shared BashOperations; env/timeout/detach absent | Stream10 and lossless raw command/cwd/path tests |
| Streaming TextDecoder, stripAnsi, sanitizeBinaryOutput, CR removal | One shared incremental decoder; existing native ANSI/sanitization authorities | Stream10 includes initial/split BOM, invalid/incomplete UTF8, ANSI/CR/binary; source never flushes the decoder at finish, neither does native |
| `outputChunks`, UTF16 `outputBytes`, max rolling size, single-large-chunk retention | Native JsString chunks/counts, drop oldest only while multiple chunks remain | Spill19 hashes complete result/callback/file bytes, including three large chunks and multibyte text |
| `ensureTempFile`: threshold uses raw bytes; replay sanitized prior chunks; line-only truncation also persists | One native file/path state, shared POSIX tmpdir, eight random bytes | Spill19: raw-NUL empty file, byte/line-only thresholds, full replay, permissions, random basename shape, TMPDIR/TMP/TEMP mutations; helper Node11 |
| onChunk after buffer/file updates; synchronous reentry | Release collector lock before callback; return existing CallbackError | Valid reentry deadlock RED→GREEN in spill19; hook/chunk5 and raw-D800 callback-after-spill full-file case |
| Success/cancelled result; ordinary error; aborted catch | Explicit close on every declared path; cancelled result suppresses the operation rejection like source | Executor6, stream10, callback-abort and ordinary-operation-error full-file histories |
| Open/write/close unhandled stream errors; late writes after end | Original native Node error dispatched after originating callback; closed/failed stream states retained | Spill19 actual syscall faults compare constructor/string/own fields and callback hashes; ordinary error still closes; late spilled writes emit ERR_STREAM_WRITE_AFTER_END |
| Simultaneous write EIO/close EBADF | Same observed source error precedence | Combined existing syscall fixture added as19th history; `bash-spill-combined-error-red.log` PASSED falsifier (not RED), so no speculative precedence fix shipped |

## Proof lineage

- Actual spill RED8: `bash-executor-spill-red-5.log`. Later valid RED6: `bash-executor-spill-red-6.log`; initial close fixtures and async file-observation race are explicitly invalid, not acceptance evidence.
- `5aeea396`: first GREEN17; `f1040d8f`: fallible callback and18th full-file history; `76cade0a`: combined-fault19th history. Direct numeric and stream suites retain their separately recorded scope.
- Native chunk callback errors retain exact UTF16 through the approved message-only ToolError boundary; unhandled IO events retain original Node error records through shared uncaught authority. No panic carrier, writer queue, fallback or production fault seam.
- Final code/proof revision and warning-free workspace check are linked from the [Bash module checkpoint](bash.md). `/tmp/pi-rust-repin-core-tools/ledger.md` records commands, exact revisions and invalidation scope.

## Explicit policy boundaries

Native file IO stays synchronous, per parent approval; unhandled event dispatch uses a native thread, not a required Tokio runtime on custom operations. Independent-producer/event timing follows the approved Node-runtime-oddity policy, not an identical-scheduling claim. Values, callback boundaries, file states and diagnostics have actual frozen proof. Node streams' default file mode follows umask; compared modes match. Typed APIs exclude invalid JavaScript runtime input types. No remaining executor-specific unapproved gap.
