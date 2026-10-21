# components/image.ts traceability

## Q2 frozen revalidation

Frozen image.ts (126 lines) audited whole. Constructor/dimension inference/defaults, options, Kitty ID allocation/reuse, max-width/max-height/cell aspect, Kitty versus iTerm blank-row/cursor-up layout, fallback, render cache and invalidate map to image.rs. `differential_components::image_render_matches_typescript` proves all three capability paths with explicit IDs. `repin_components::image_fallback_is_not_truncated_or_hyperlinked_by_the_component` proves full styled literal filename fallback even at width4 and hyperlinks enabled. Numeric u32/nonfinite restrictions are approved; see [terminal-image](terminal-image.md). Earlier claims of fallback truncation are obsolete. Q2 removed the truncation and preserved frozen truthiness for result imageId. [Proof](../q2-tui-components-proof.md).


Source: `packages/tui/src/components/image.ts` (127 lines). Rust:
`crates/pi-tui/src/components/image.rs`.

Tests: `crates/pi-tui/tests/differential_components.rs::image_render_matches_typescript`
(kitty, iTerm2, no-capability fallback, unknown mime fallback).

## Exported symbols

| TypeScript export | Rust item | Tests |
|---|---|---|
| `interface ImageTheme` | `ImageTheme { fallback_color }` | differential |
| `interface ImageOptions` | `ImageOptions` | differential |
| `class Image` | `Image` | differential |
| `constructor(base64Data, mimeType, theme, options = {}, dimensions?)` | `Image::new(...)` | differential |
| `getImageId` | `image_id()` | differential |
| `invalidate` | `Component::invalidate` | not directly asserted |
| `render(width)` | `Component::render` | differential |

## Branches

| TypeScript branch | Rust branch | Tests |
|---|---|---|
| dimensions fallback `{800,600}` when undetectable | `unwrap_or(800x600)` | differential (dimensions passed explicitly) |
| max width `max(1, min(width-2, maxWidthCells ?? 60))` | same | differential |
| default max height from cell aspect ratio | `ceil(max_width * cell_w / cell_h)` | differential |
| capabilities present + kitty + no id → allocate | `allocate_image_id` | gap: id is random, differential uses explicit ids |
| `renderImage` failure → fallback line | `None` branch | differential (kitty with an unknown mime) |
| kitty lines: sequence + `rows-1` blanks | same | differential |
| iTerm2 lines: blanks + cursor-up + sequence | same | differential |
| no capabilities → fallback truncated to width | `fallback_line` | differential |
| render cache by width | `cached_lines/cached_width` | differential repeated render |

## Behavioral notes

- Kitty image IDs are random in both implementations, so differential cases pass
  explicit IDs; `getImageId` is still compared for explicit and iterm2 cases.
- `imageFallback` output and truncation are reused from `terminal-image.rs`,
  already differentially covered.

## Gaps / exclusions

- Automatic kitty ID allocation is not compared (non-deterministic); covered
  indirectly by the `image_id()` accessor.
- JavaScript callers passing negative/fractional cell sizes are excluded.

## Discrepancies

None found.
