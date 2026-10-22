# spacer.ts — Q2 frozen revalidation

Frozen `4f9e6f8a4`, 28 lines. Rust: `components/spacer.rs`. Spacer constructor/default lines1, setLines, no-op invalidate, render blank-line count map directly to Rust. Typed usize excludes negative/nonfinite JS counts. Native `spacer` and `differential_components::spacer_dynamic_border_and_truncated_text_match_typescript` pass. [Q2 proof](../q2-tui-components-proof.md).

## Q2 findings correction

`Spacer::default()` renders one empty row; explicit `new(0)` renders zero. Frozen default/zero differential: `spacer_default_and_explicit_zero_match_typescript`.
