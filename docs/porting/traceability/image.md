# components/image.ts traceability

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
