# stdin-buffer.ts traceability

Source: `packages/tui/src/stdin-buffer.ts` (444 lines). Rust: `crates/pi-tui/src/stdin_buffer.rs`.

Differential oracle: `oracle/tui-leaves-oracle.ts` (`stdin_buffer`). Tests: `crates/pi-tui/tests/stdin_buffer.rs`, `crates/pi-tui/tests/differential_stdin_buffer.rs`.

## Exported symbols

| TypeScript export | Rust item | Tests |
|---|---|---|
| `type StdinBufferOptions` | `struct StdinBufferOptions { timeout, escape_timeout }` | timeout tests |
| `type StdinBufferEventMap` | `enum StdinBufferEvent { Data, Paste }` | all tests |
| `class StdinBuffer` | `struct StdinBuffer` | all tests |
| `constructor(options?)` | `new()` / `with_options(options)` | all tests |
| `process(data: string \| Buffer)` | `process(&str)` / `process_bytes(&[u8])` | unit tests; differential strings |
| `flush()` | `flush() -> Vec<String>` | `explicit_flush_returns_and_clears_buffered_sequences` |
| `clear()` | `clear()` | `clear_drops_buffered_content_without_emitting` |
| `getBuffer()` | `buffer() -> &str` | all partial-sequence tests |
| `destroy()` | `destroy()` | `destroy_clears_buffer_and_pending_timeout` |
| `on("data" \| "paste")` | returned `Vec<StdinBufferEvent>` | all tests |
| `setTimeout` flush | `next_deadline()` + `poll(now)` | timeout tests |

## Branches

| TypeScript branch | Rust branch | Tests |
|---|---|---|
| empty chunk + empty buffer → `data("")` | `emit_data_sequence("")` | `handles_empty_input` |
| Buffer single byte > 127 → `ESC + (byte - 128)` | `process_bytes` | `handles_buffer_input_and_high_byte_conversion` |
| paste mode accumulation / end marker | `process_paste_chunk` | paste tests |
| paste start, content before/after | `begin_paste` | paste tests |
| complete sequence extraction | `extract_complete_sequences` | all sequence tests |
| incomplete remainder + timeout | `deadline` + `poll` | timeout tests |
| lone ESC uses `escapeTimeout` | `buffer == ESC` branch | escape timeout tests |
| WezTerm `ESC ESC [ ...` split | `candidate == "\x1b\x1b"` + next-byte check | `splits_esc_esc_csi_into_standalone_escape_and_csi` |
| duplicate raw char after Kitty CSI-u | `pending_kitty_printable_codepoint` | `drops_raw_duplicate_characters_after_kitty_printable_sequences` |
| OSC/DCS/APC termination (BEL/ST) | `is_complete_terminated_sequence` | unit + differential |
| CSI final-byte 0x40..=0x7E | `is_complete_csi_sequence` | unit + differential |
| SGR mouse payload validation | `is_sgr_mouse_payload` | mouse tests |
| old-style mouse `ESC[M` + 3 bytes | `utf16_len(data) >= 6` | old-style mouse tests |
| SS3 `ESC O` + char | `after_escape.starts_with('O')` | differential |
| meta key `ESC` + char | `utf16_len(after_escape) == 1` | differential |

## Behavioral notes

- **Timer ownership deviation (API adaptation, same behavior):** TypeScript owns a `setTimeout` and emits flushed data when it fires. Rust exposes `next_deadline()` and `poll(now)`; the terminal driver sleeps until the deadline and calls `poll`. `poll` flushes and emits exactly like the timeout callback. Differential tests compare the post-timeout event stream against the TypeScript timer.
- `StdinBuffer::new()` uses the source defaults (50 ms sequence timeout, 10 ms escape timeout).
- `process_bytes` reproduces `Buffer.isBuffer` high-byte conversion and otherwise decodes UTF-8 with replacement (`Buffer.toString()`).
- Buffers and indices are byte-based; sequences are ASCII-framed, so behavior matches the UTF-16-indexed source for all tested inputs.

## Gaps / exclusions

- The source emits non-escape text one UTF-16 code unit at a time. For astral characters this yields two lone-surrogate events, which Rust `String` cannot represent. Rust emits one event per Unicode scalar value. This is a typed-API exclusion (decision 2026-08-12) and affects only astral characters outside escape sequences. Differential cases use BMP text and whole paste payloads (paste content is emitted as one event in both).
- JavaScript callers passing non-string/non-Buffer values are excluded by the typed API.

## Discrepancies

None found in the ported behavior. The source comment in `isCompleteOscSequence` is accurate; the Rust helper also serves DCS/APC with their respective prefixes.
