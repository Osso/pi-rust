# RPC JSONL traceability

Current source: `packages/coding-agent/src/modes/rpc/jsonl.ts` at [common pin4f9e6f8a4](../source-lock.json). The client slice revalidates its bounded byte-stream transport errors/lifetime below; it does not revalidate the older serializer/decoded-string exclusions.

## Current transport boundary

| Branch | Exact behavior | Common-pin proof |
|---|---|---|
| Read failure | Original Node read Error name/message/code/errno/syscall reaches shared uncaught authority, not a successful EOF or silently dropped task result | Actual-source/native `rpc_jsonl_errors`2/2 |
| onLine throw | `Result<(), CallbackError>` rejection is uncaught at data/end delivery; the client's separate handleLine catch is not applied here | Same source/native error target; original callback TypeError/raw message retained |
| Detach | Disable later callback/final-line delivery, but keep draining/discarding the live pipe; already-entered chunk emits its remaining lines | Reader5/5 and real writing-child EPIPE RED→GREEN in client differential16/16 |
| Signal disposal consumer | Intentional reader detach is not real stdin EOF while stdout flush is blocked | Full mode30/30, including SIGHUP exit129 and held512KiB flush |

See [pair proof](../rpc-client-proof.md) for commands/revisions/artifacts. No serializer policy or UTF-16 string-chunk gap is newly approved here.

## Historical framing audit (not new-pin acceptance)

The table below records `532f7d12cd19235081bd4d6e425bde185652d86f` proof only. Its exclusions remain pending revalidation under AGENTS.

Direct differential proof invokes the real pinned functions through one persistent `tsx` process. It compares six shared serialization fixtures (`9000..=9005`), pins four approved serializer discrepancies (`9006..=9009`), compares eleven ordered decoder sequences (`9010..=9020`), and proves three matching non-finite cases (`9021..=9023`).

| TypeScript source | Observable behavior | Rust proof |
|---|---|---|
| `jsonl.ts:1-2` | Uses Node streams plus stateful UTF-8 `StringDecoder` | Transport-neutral `JsonlLineDecoder` holds the decoding state; `attach_jsonl_line_reader` owns one decoder per stream |
| `jsonl.ts:5-12` | `JSON.stringify(value)` followed by exactly one LF | Focused compact JSON, primitive, escaped-LF, U+2028, U+2029, error-propagation, non-finite, and approved Rust-number-spelling tests; six exact-output fixtures, four pinned discrepancies, and three matching NaN/infinity fixtures |
| `jsonl.ts:21-23` | One UTF-8 decoder and one text buffer persist across data chunks | Focused arbitrary chunking and split two-byte/four-byte scalar tests plus eleven persistent-state differential sequences |
| `jsonl.ts:24-27` | Strip exactly one trailing CR immediately before emitting each line | Focused CRLF, repeated-CR, bare/internal-CR, and final trailing-CR tests plus direct CR/empty-record comparison |
| `jsonl.ts:29-30` | String chunks append directly; byte chunks pass through `StringDecoder.write` | Focused and direct mixed pending-byte/string-chunk ordering fixture |
| `jsonl.ts:32-41` | Repeatedly split only on LF and preserve record order | Focused and direct LF-only, multiple-record, empty-record, Unicode-separator, terminal-LF, and arbitrary-chunk fixtures |
| `jsonl.ts:43-49` | Flush incomplete UTF-8 at end and emit one non-empty final unterminated line | Focused and direct incomplete-sequence replacement, final-line, terminal-LF, and empty-stream fixtures |
| `jsonl.ts:21-57` | `attachJsonlLineReader` attaches `data`/`end` listeners and returns a detach closure | `attach_jsonl_line_reader(AsyncRead, FnMut(String) -> Result<(), CallbackError>) -> JsonlLineReader` drives the proven decoder on a Tokio task; `detach()` stops at the next chunk boundary (matching the source, where a detach inside `onLine` still lets the current chunk's lines emit) and suppresses the final unterminated line; `finished()` awaits the underlying stream end; read/callback failures use uncaught authority and detached reads keep draining. `tests/rpc_jsonl_reader.rs` ports all three stream cases of `test/rpc-jsonl.test.ts` (U+2028/U+2029 payload, CRLF, final line) plus split-UTF-8-across-reads and detach cases |
| Node `StringDecoder("utf8")` | Buffer incomplete sequences; replace malformed/incomplete bytes; preserve BOM | Focused and direct split-scalar, invalid leading/continuation, overlong, surrogate, incomplete-end, BOM, and empty-chunk fixtures |

Approved discrepancy, August 12, 2026: `serialize_json_line` uses exact-pinned `serde_json` for semantic JSON interoperability. It does not reproduce `JSON.stringify` byte spelling for integral `f64`, negative zero, exponent thresholds, or Rust integer values without exact JavaScript Number equivalents. Tests pin the Rust output; differential probes must pin the corresponding TypeScript output instead of asserting equality for those classified cases.
