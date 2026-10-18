# dynamic-border.ts

Source: `coding-agent/src/modes/interactive/components/dynamic-border.ts`
Rust: `crates/pi-coding-agent/src/modes/interactive/components/dynamic_border.rs`

| Symbol / branch | Rust item | Proof |
|---|---|---|
| default color `theme.fg("border", line)` | `dynamic_border(&theme)` | all differential component renders (borders) |
| custom color callback | `dynamic_border_with` | `earendil-announcement` behavioral test (accent borders) |

`pi-tui::DynamicBorder` provides the render behavior (already differentially tested in `pi-tui`).
