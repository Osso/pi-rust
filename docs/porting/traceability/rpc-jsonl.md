# RPC JSONL traceability

Pinned source: `packages/coding-agent/src/modes/rpc/jsonl.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Focused proof uses typed Rust values and explicit byte chunks. Direct TypeScript differential proof is pending for this production commit.

| TypeScript source | Observable behavior | Rust proof |
|---|---|---|
| `jsonl.ts:1-2` | Uses Node streams plus stateful UTF-8 `StringDecoder` | Transport-neutral Rust decoder retains only the observable text-decoding state; stream attachment remains out of scope |
| `jsonl.ts:5-12` | `JSON.stringify(value)` followed by exactly one LF | Focused compact JSON, primitive, escaped-LF, U+2028, and U+2029 tests; stock `serde_json` number spelling is an approved deviation |
| `jsonl.ts:21-22` | One UTF-8 decoder and one text buffer persist across data chunks | Focused arbitrary chunking and split two-byte/four-byte scalar tests |
| `jsonl.ts:24-27` | Strip exactly one trailing CR immediately before emitting each line | Focused CRLF, repeated-CR, bare/internal-CR, and final trailing-CR tests |
| `jsonl.ts:29-30` | String chunks append directly; byte chunks pass through `StringDecoder.write` | Focused mixed pending-byte/string-chunk ordering test |
| `jsonl.ts:32-41` | Repeatedly split only on LF and preserve record order | Focused LF-only, multiple-record, consecutive-empty-record, terminal-LF, and character-at-a-time tests |
| `jsonl.ts:43-49` | Flush incomplete UTF-8 at end and emit one non-empty final unterminated line | Focused incomplete-sequence replacement, final-line, terminal-LF, and empty-stream tests |
| `jsonl.ts:51-57` | Attach `data`/`end` listeners and return a closure that removes them | Explicit gap: process/stream I/O and detach ownership remain a later RPC integration slice |
| Node `StringDecoder("utf8")` | Buffer incomplete sequences; replace malformed/incomplete bytes; preserve BOM | Focused split-scalar, invalid leading/continuation, overlong, surrogate, incomplete-end, BOM, and empty-chunk fixtures |

Approved discrepancy, August 12, 2026: `serialize_json_line` uses exact-pinned `serde_json` for semantic JSON interoperability. It does not reproduce `JSON.stringify` byte spelling for integral `f64`, negative zero, exponent thresholds, or Rust integer values without exact JavaScript Number equivalents. Tests pin the Rust output; differential probes must pin the corresponding TypeScript output instead of asserting equality for those classified cases.
