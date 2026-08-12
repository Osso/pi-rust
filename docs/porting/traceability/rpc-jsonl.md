# RPC JSONL traceability

Pinned source: `packages/coding-agent/src/modes/rpc/jsonl.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Direct differential proof invokes the real pinned functions through one persistent `tsx` process. It compares six shared serialization fixtures (`9000..=9005`), pins four approved serializer discrepancies (`9006..=9009`), compares eleven ordered decoder sequences (`9010..=9020`), and proves three matching non-finite cases (`9021..=9023`).

| TypeScript source | Observable behavior | Rust proof |
|---|---|---|
| `jsonl.ts:1-2` | Uses Node streams plus stateful UTF-8 `StringDecoder` | Transport-neutral Rust decoder retains only the observable text-decoding state; stream attachment remains out of scope |
| `jsonl.ts:5-12` | `JSON.stringify(value)` followed by exactly one LF | Focused compact JSON, primitive, escaped-LF, U+2028, U+2029, error-propagation, non-finite, and approved Rust-number-spelling tests; six exact-output fixtures, four pinned discrepancies, and three matching NaN/infinity fixtures |
| `jsonl.ts:21-22` | One UTF-8 decoder and one text buffer persist across data chunks | Focused arbitrary chunking and split two-byte/four-byte scalar tests plus eleven persistent-state differential sequences |
| `jsonl.ts:24-27` | Strip exactly one trailing CR immediately before emitting each line | Focused CRLF, repeated-CR, bare/internal-CR, and final trailing-CR tests plus direct CR/empty-record comparison |
| `jsonl.ts:29-30` | String chunks append directly; byte chunks pass through `StringDecoder.write` | Focused and direct mixed pending-byte/string-chunk ordering fixture |
| `jsonl.ts:32-41` | Repeatedly split only on LF and preserve record order | Focused and direct LF-only, multiple-record, empty-record, Unicode-separator, terminal-LF, and arbitrary-chunk fixtures |
| `jsonl.ts:43-49` | Flush incomplete UTF-8 at end and emit one non-empty final unterminated line | Focused and direct incomplete-sequence replacement, final-line, terminal-LF, and empty-stream fixtures |
| `jsonl.ts:51-57` | Attach `data`/`end` listeners and return a closure that removes them | Explicit gap: process/stream I/O and detach ownership remain a later RPC integration slice |
| Node `StringDecoder("utf8")` | Buffer incomplete sequences; replace malformed/incomplete bytes; preserve BOM | Focused and direct split-scalar, invalid leading/continuation, overlong, surrogate, incomplete-end, BOM, and empty-chunk fixtures |

Approved discrepancy, August 12, 2026: `serialize_json_line` uses exact-pinned `serde_json` for semantic JSON interoperability. It does not reproduce `JSON.stringify` byte spelling for integral `f64`, negative zero, exponent thresholds, or Rust integer values without exact JavaScript Number equivalents. Tests pin the Rust output; differential probes must pin the corresponding TypeScript output instead of asserting equality for those classified cases.
