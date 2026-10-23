# terminal-colors.ts frozen traceability

Reference: immutable `4f9e6f8a4`, `packages/tui/src/terminal-colors.ts` (73 lines).
Rust: `crates/pi-tui/src/terminal_colors.rs`. [Q1 proof](../q1-tui-core-proof.md).

| Export/function/branch | Rust mapping | Proof |
|---|---|---|
| `RgbColor` | f64 channel struct including NaN/Infinity | unit numeric-domain test, terminal/parser/query/theme differentials |
| `TerminalColorScheme` | Dark/Light enum | unit and differential |
| `hexToRgb` | validated 6-digit conversion | black/white/mixed-case fixtures |
| `parseOscHexChannel` | validated hex, shared JS parseInt/Math.round, no nonfinite filtering | 1/2/3/4/16/17/32/100/255/256/257/300/1024-digit channel cases |
| OSC11 strict envelope | `osc11_value`, `is_osc11_background_color_response` | prefix/suffix contamination, embedded BEL/ESC, BEL/ST endings |
| `parseOsc11BackgroundColor` | same-name parser | JS trimming; 6/12-digit hash colors; rgb/rgba case-insensitive prefixes; missing/invalid channels; ignored extra channel |
| `parseTerminalColorSchemeReport` | one exact dark/light frame | repeated, empty, contaminated and unknown reports rejected |

Frozen color-scheme regex accepts exactly one report. The later repeated-report/last-capture behavior was removed after differential RED. Long finite hex channels use floating arithmetic instead of overflowing u64 exponentiation.

## Full numeric domain (supersedes pre-repin restriction)

Frozen :21–25 can produce NaN for unbounded valid hex channels: the 300-f report reaches actual TUI query and then the theme detector. Parent cleared full f64 widening2026-10-23 after this built-in consumer proof; no NaN exclusion remains. Native preserves all f64 channels, and theme diagnostic text uses JS number formatting (Infinity, NaN, negative zero).

`differential_terminal_colors` expands the prior source corpus beyond255 digits. `differential_tui::frozen_background_query_preserves_nonfinite_channels` proves actual pending query settlement. `pi-coding-agent/tests/differential_theme_rgb.rs` compares actual frozen parser→detector output plus direct fractional/negative/nonfinite inputs. Source returns dark with high-confidence terminal background and `OSC 11 background rgb(NaN, 0, 0)` for overflow; that literal behavior is retained.

Artifacts `/tmp/pi-rust-repin-lossless-interactive`: `rgb-terminal-red.log` reproduces prior None vs Node RGB object; `rgb-terminal-green.log`17 tests; `rgb-tui-query-green.log`1 process query scenario; `rgb-theme-green.log`8 detector vectors.
