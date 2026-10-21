# terminal-colors.ts frozen traceability

Reference: immutable `4f9e6f8a4`, `packages/tui/src/terminal-colors.ts` (73 lines).
Rust: `crates/pi-tui/src/terminal_colors.rs`. [Q1 proof](../q1-tui-core-proof.md).

| Export/function/branch | Rust mapping | Proof |
|---|---|---|
| `RgbColor` | byte RGB struct | unit and differential |
| `TerminalColorScheme` | Dark/Light enum | unit and differential |
| `hexToRgb` | validated 6-digit conversion | black/white/mixed-case fixtures |
| `parseOscHexChannel` | validated hex, floating exponent/parse/round | 1/2/3/4/16/17/32/100/255-digit channel cases |
| OSC11 strict envelope | `osc11_value`, `is_osc11_background_color_response` | prefix/suffix contamination, embedded BEL/ESC, BEL/ST endings |
| `parseOsc11BackgroundColor` | same-name parser | JS trimming; 6/12-digit hash colors; rgb/rgba case-insensitive prefixes; missing/invalid channels; ignored extra channel |
| `parseTerminalColorSchemeReport` | one exact dark/light frame | repeated, empty, contaminated and unknown reports rejected |

Frozen color-scheme regex accepts exactly one report. The later repeated-report/last-capture behavior was removed after differential RED. Long finite hex channels use floating arithmetic instead of overflowing u64 exponentiation.

## Approved numeric-domain deviation (2026-10-21)

Frozen source lines 21/25 can produce NaN: `rgb:` with 256 `f` digits, 256 `8` digits and 256 `0` digits yields `{r:NaN,g:0,b:0}` (JSON `{r:null,g:0,b:0}`). The byte RGB API cannot represent NaN; Rust returns None. User approved retaining this numeric-domain gap on 2026-10-21. No RGB API widening; the module is done within this approved restriction. Exact frozen capture: `/tmp/pi-rust-repin-tui-core/nonfinite-rgb.log`.
