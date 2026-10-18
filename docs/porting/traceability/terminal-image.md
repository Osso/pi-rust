# terminal-image.ts traceability

Source: `packages/tui/src/terminal-image.ts` (657 lines). Rust: `crates/pi-tui/src/terminal_image.rs`, internal Base64 support in `crates/pi-tui/src/base64.rs`.

Differential oracle: `oracle/tui-leaves-oracle.ts` (`terminal_image_*`). Tests: `crates/pi-tui/tests/terminal_image.rs`, `crates/pi-tui/tests/differential_terminal_image.rs`.

## Exported symbols

| TypeScript export | Rust item | Tests |
|---|---|---|
| `type ImageProtocol` | `enum ImageProtocol { Kitty, Iterm2 }` (`None` = no images) | capability tests |
| `interface TerminalCapabilities` | `struct TerminalCapabilities` | capability tests, differential |
| `interface CellDimensions` | `struct CellDimensions` | all image tests |
| `interface ImageDimensions` | `struct ImageDimensions` | all image tests |
| `interface ImageRenderOptions` | `struct ImageRenderOptions` | render tests |
| `getCellDimensions()` | `get_cell_dimensions()` | `default_cell_dimensions_match_the_source` |
| `setCellDimensions(dims)` | `set_cell_dimensions(dims)` | render/calc tests |
| `detectCapabilities(fn?)` | `detect_capabilities()` / `detect_capabilities_in(env, fn)` | capability matrix, differential |
| `getCapabilities()` | `get_capabilities()` | render tests |
| `resetCapabilitiesCache()` | `reset_capabilities_cache()` | render tests |
| `setCapabilities(caps)` | `set_capabilities(caps)` | render tests |
| `isImageLine(line)` | `is_image_line(&str)` | line tests, differential |
| `allocateImageId()` | `allocate_image_id() -> u32` | `allocate_image_id_stays_in_range` |
| `encodeKitty(data, options?)` | `encode_kitty(&str, &KittyEncodeOptions)` | unit + differential (short/chunked) |
| `deleteKittyImage(id)` | `delete_kitty_image(u32)` | unit + differential |
| `deleteAllKittyImages()` | `delete_all_kitty_images()` | unit + differential |
| `deleteAllKittyPlacements()` | `delete_all_kitty_placements()` | unit + differential |
| `encodeITerm2(data, options?)` | `encode_iterm2(&str, &Iterm2Options)` | unit + differential |
| `interface ImageCellSize` | `struct ImageCellSize` | calc tests |
| `interface KittyImageMetadata` | `struct KittyImageMetadata` | metadata tests |
| `interface KittyImagePlacement` | `struct KittyImagePlacement` | placement tests |
| `registerKittyImageMetadata(metadata)` | `register_kitty_image_metadata(metadata)` | metadata/placement/eviction tests |
| `getKittyImageMetadata(line)` | `get_kitty_image_metadata(&str)` | metadata tests |
| `getKittyImagePlacement(line)` | `get_kitty_image_placement(&str)` | placement test + differential |
| `cropKittyImageLine(line, hidden, visible)` | `crop_kitty_image_line(&str, i64, i64)` | crop test + differential |
| `calculateImageCellSize(...)` | `calculate_image_cell_size(...)` | calc tests + differential |
| `calculateImageRows(...)` | `calculate_image_rows(...)` | calc test |
| `getPngDimensions(data)` | `get_png_dimensions(&str)` | unit + differential |
| `getJpegDimensions(data)` | `get_jpeg_dimensions(&str)` | unit + differential |
| `getGifDimensions(data)` | `get_gif_dimensions(&str)` | unit + differential |
| `getWebpDimensions(data)` | `get_webp_dimensions(&str)` | unit + differential |
| `getImageDimensions(data, mime)` | `get_image_dimensions(&str, &str)` | unit + differential |
| `renderImage(data, dims, options?)` | `render_image(...) -> Option<RenderedImage>` | render tests + differential |
| `hyperlink(text, url)` | `hyperlink(&str, &str)` | unit + differential |
| `imageFallback(mime, dims?, filename?)` | `image_fallback(&str, Option<ImageDimensions>, Option<&str>)` | unit + differential |

## Branches

| TypeScript branch | Rust branch | Tests |
|---|---|---|
| tmux / TERM=tmux → no images, probe hyperlinks | `detect_capabilities_in` first branch | capability matrix + differential |
| screen → no hyperlinks | `term.starts_with("screen")` | unit + differential |
| Kitty, Ghostty, WezTerm, Warp detection | ordered branches | unit + differential |
| iTerm2, Windows Terminal, VSCode, Alacritty | ordered branches | unit + differential |
| JetBrains, Windows console, unknown terminal | ordered branches | unit + differential |
| color-term truecolor/24bit hint | `has_true_color_hint` | unit + differential |
| Kitty chunk size 4096, first/middle/last `m=` flags | `encode_kitty` | unit + differential (8192 payload) |
| iTerm2 `inline`, `size`, width/height, name, preserveAspectRatio | `encode_iterm2` | unit + differential |
| image ID registration, generation bump, delete-before-insert, 1000-entry eviction | `register_kitty_image_metadata` | metadata + eviction tests |
| placement multi-chunk scan and control filtering | `get_kitty_image_placement` | placement test + differential |
| crop row math (`floor`/`ceil`/clamp), control replacement | `crop_kitty_image_line` | crop test + differential |
| cell-size scaling, max width/height clamps | `calculate_image_cell_size` | calc tests + differential |
| PNG/JPEG/GIF/WebP (VP8, VP8L, VP8X) parsing | dimension functions | unit + differential |
| render Kitty/iTerm2/no images, cursor movement | `render_image` | unit + differential |
| fallback shortening + OSC 8 file link | `image_fallback`, `shorten_image_path`, `path_to_file_url` | unit + differential |

## Behavioral notes

- Capability detection reads a `TerminalEnvironment` snapshot instead of `process.env` directly. `TerminalEnvironment::from_process()` supplies the process values; the explicit variant exists so tests and the differential oracle can exercise the matrix without mutating global process state. Behavior matches the source branch order exactly.
- `probeTmuxHyperlinks` runs `tmux display-message -p '#{client_termfeatures}'` with a 250 ms bounded wait (spawn + poll + kill), then checks for a trimmed `hyperlinks` feature. Errors return `false`.
- Kitty IDs are allocated from a `RandomState`-derived value in `[1, 0xfffffffe]`; the source uses `Math.random`. Randomness is not differential-tested; the range is unit-tested.
- `Buffer.from(base64)` semantics are reproduced by `base64::decode_lenient`: invalid characters are skipped and decoding stops at the first `=`. `Buffer.byteLength(base64, "base64")` is reproduced by `base64::decoded_len`.
- `pathToFileURL` is reproduced for absolute POSIX paths with percent-encoding of space, `#`, `?`, `%`, and non-ASCII bytes; unreserved and sub-delim characters stay literal (matching Node for tested inputs).
- `getCellDimensions`/`setCellDimensions` and capability cache are process-global, as in the source; tests serialize access with a mutex.

## Gaps / exclusions

- `probeTmuxHyperlinks` is platform/tool dependent and is not differential-tested (no tmux in the test environment). The injected `tmuxForwardsHyperlink` argument of `detectCapabilities` is fully covered.
- `allocateImageId` randomness cannot be differentially compared; only the numeric range is asserted.
- `pathToFileURL` is limited to POSIX paths; Windows drive-letter and UNC handling from Node is not ported (the Rust port targets Linux terminals).
- JavaScript callers passing non-number options or fractional pixel dimensions are excluded by the typed API. Cell-count maxima remain `f64` because the source floors fractional values.
- `Image` component rendering (`components/image.ts`) is out of this leaf slice.

## Discrepancies

- `getWebpDimensions` starts with `if (buffer.length < 30) return null`, which makes the inner VP8L `if (buffer.length < 25) return null` branch unreachable. The Rust port preserves the effective `< 30` requirement (verified against the source oracle with a 25-byte VP8L payload returning `null` and a 30-byte payload decoding successfully). Classified as a source bug, preserved.
- No other discrepancies found.
