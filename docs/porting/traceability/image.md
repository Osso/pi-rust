# components/image.ts traceability

Frozen authority:126-line `packages/tui/src/components/image.ts`. Rust: `crates/pi-tui/src/components/image.rs`. [Numeric contract](../../specs/image-numeric-options.md); [terminal helpers](terminal-image.md).

| Export / branch | Proof |
| --- | --- |
| ImageTheme, ImageOptions, Image constructor, explicit dimensions/default inference | existing `differential_components::image_render_matches_typescript` |
| maxWidthCells/maxHeightCells number semantics; fractional default height; negative/NaN/±Infinity | `differential_image_component_numbers`:420 actual frozen cases, all three image capabilities, widths0/1/2/6/80 |
| render cache and invalidate; capabilities changed behind cache | same420 cases compare cached renders, later invalidate and width change; fallback callback side effects included |
| Kitty/iTerm2 row layout and explicit image ID | same corpus plus existing image differential; NaN sizes omit falsy Kitty c/r parameters, iTerm2 serializes NaN literally |
| full untruncated fallback/filename and raw UTF-16 | existing `repin_components` and `raw_images` |
| random Kitty ID allocation | native range/retention proof, not RNG identity across processes |

RED `test-image-numeric-red-1.log`:100/420 cases differed. GREEN `test-image-numeric-green-1.log`:420/420 match. Negative/fractional/nonfinite **cell options** are no longer excluded. Cell/image pixel dimensions and explicit image IDs retain the separate approved typed u32 boundary. Native render width remains the shared Component usize boundary.

Image options, calculated cell dimensions, rendered-image metadata and Kitty cell options carry f64 end-to-end; only loop iteration counts become usize after the frozen numeric comparisons. Shared JS min/max preserve NaN and signed zero. No source discrepancy discovered in this slice; no whole Q13 completion claimed.
