# stdin-buffer.ts frozen traceability

Reference: immutable `4f9e6f8a4`, `packages/tui/src/stdin-buffer.ts` (434 lines).
Rust: `crates/pi-tui/src/stdin_buffer.rs`. [Q1 proof](../q1-tui-core-proof.md).

| Export/function/branch | Rust mapping | Current proof |
|---|---|---|
| `StdinBufferOptions`, constructor | one `timeout: Duration`, default 10ms | default partial-CSI and custom-100ms lone-ESC differential RED/GREEN |
| `StdinBufferEventMap`, `on(data/paste)` | returned `Vec<StdinBufferEvent>` | chunk/paste differential and 28 unit cases |
| `process(string/Buffer)` | `process`, `process_bytes` | string/BMP chunks; high-byte Buffer conversion unit cases |
| `isCompleteSequence` | `is_complete_sequence` | CSI, old mouse, OSC, DCS, APC, SS3, meta, incomplete branches |
| `isCompleteCsiSequence` | same-name Rust helper | final-byte range and SGR mouse validation |
| OSC/DCS/APC termination | `is_complete_terminated_sequence` | BEL only for OSC; ST for all three |
| `parseUnmodifiedKittyPrintableCodepoint` | same-name Rust helper | alternate-codepoint grammar and duplicate raw printable suppression |
| `extractCompleteSequences` | same-name Rust helper | batched text/control sequences and WezTerm ESC ESC restart |
| paste begin/accumulate/end, content before/after | `begin_paste`, `process_paste_chunk` | unit + differential event ordering |
| `emitDataSequence` | same-name Rust helper | duplicate suppression reset on intervening data/paste/flush |
| `flush`, `clear`, `getBuffer`, `destroy` | same methods (`buffer` getter) | explicit flush, no-emission clear, pending-timeout destruction |
| timeout emission | `next_deadline` + `poll` | real frozen timer differential, deterministic Rust deadline polling |

Removed later `escapeTimeout`/`escape_timeout` option and 50ms sequence default. Frozen applies the same timeout to lone ESC and all other incomplete sequences.

Representation boundary: Rust owns timer driving and returns events instead of using EventEmitter. Rust String cannot contain lone surrogates; frozen emits non-escape astral text as individual UTF-16 units, whereas Rust emits scalar strings. This existing typed-string boundary is not claimed as byte parity. Whole paste payloads preserve astral text in both implementations. Invalid JS runtime input types are outside the typed Rust API.
