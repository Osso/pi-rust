# coding-agent ansi traceability

Pinned source: `packages/coding-agent/src/utils/ansi.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Differential proof: request `26001` executes the real pinned module and compares 237 generated compatibility inputs (the source test corpus: fixed cases plus ESC/C1 combinations sampled every third character). Focused tests: `crates/pi-coding-agent/tests/ansi.rs`.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `ansi.ts:29-42` | OSC alternative: `ESC ]` to the first BEL, `ESC \`, or `0x9c`; CSI alternative: ESC/C1, intermediates `[]()#;?`, optional decimal params with `;`/`:`, final byte set | `utils::ansi::ansi_regex` compiles the same pattern with ASCII `[0-9]` digit classes; differential request `26001` |
| `ansi.ts:46-49` | Non-string input throws `TypeError` | Approved typed-boundary exclusion: Rust accepts `&str` only |
| `ansi.ts:51-53` | Fast path returns input unchanged without ESC or C1 CSI | Focused `preserves_text_without_escape_introducers` |
| `ansi.ts:59` | Global replacement strips every match | Focused 18-case corpus plus differential 237-case corpus |

## Discrepancies

- JavaScript `\d` is ASCII-only; the Rust pattern uses `[0-9]` to preserve that.
- Non-string runtime inputs are excluded by the typed Rust API.

## Proof inventory

- Focused cases: 18 fixed inputs, single-byte ESC finals, no-introducer fast path.
- Differential request: `26001`, 237 generated inputs.
