# terminal-image.ts — Q2 frozen re-pin

Authority: [4f9e6f8a4](../source-lock.json), 488 lines. Rust: `crates/pi-tui/src/terminal_image.rs`. Contract/proof: [components](../../specs/tui-components.md), [Q2](../q2-tui-components-proof.md).

| Frozen exports / branches | Rust / proof |
|---|---|
| ImageProtocol, TerminalCapabilities, CellDimensions, ImageDimensions, ImageRenderOptions, ImageCellSize | typed Rust structs/enums; dimensions remain u32 |
| get/setCellDimensions; get/set/resetCapabilitiesCache | process-global mutex state; terminal_image native/differential and Image component tests |
| detectCapabilities; probeTmuxHyperlinks | ordered tmux/screen/Kitty/Ghostty/WezTerm/Warp/iTerm/WT/VSCode/Alacritty/JetBrains/default matrix; `differential_terminal_image`; private real executable exit0/exit1 probe in `terminal_image::tmux_probe_requires_a_successful_exit` |
| isImageLine | beginning/embedded/very-long/nonimage lines; native plus frozen differential |
| allocateImageId | nondeterministic allocation; range 1..=0xfffffffe native assertion, not cross-process RNG identity |
| encodeKitty | flags, optional dimensions/id/C=1, empty/single/chunked data; native plus frozen differential |
| deleteKittyImage; deleteAllKittyImages | exact control bytes native/frozen differential; forced redraw regression proves retained IDs are deleted before full clear |
| encodeITerm2 | inline/width/height/name/aspect; **no size field**; JS numeric string formatting including exponent/-0; native/frozen differential |
| calculateImageCellSize; calculateImageRows | width/height floor/clamp/aspect and custom cells; native/frozen differential within approved numeric domain |
| getPng/Jpeg/Gif/WebpDimensions; getImageDimensions | all formats, short/invalid buffers, JPEG markers, WebP VP8/VP8L/VP8X; Node ASCII high-bit header masking included in native/frozen differential |
| renderImage | Kitty/iTerm2/no-images, bounds, moveCursor, imageId; native/frozen differential |
| hyperlink; imageFallback | exact OSC8 and literal filename/MIME/dimensions; fallback never shortens or hyperlinks paths; native/frozen differential and Image fallback regression |

Removed because absent frozen source: metadata/placement/crop registry, deleteAllKittyPlacements, Windows-console capability branch, path shortening, size metadata. `path_to_file_url` moved to coding-agent render-utils, whose frozen source imports Node's URL helper. It is no longer terminal-image API.

## Representations and approved restrictions

- Cell/image dimensions and image IDs remain u32; JavaScript negative/fractional/nonfinite pixel inputs cannot be represented. Above-u32 and nonfinite/zero-cell calculation outputs remain out of range under the user's October 21, 2026 numeric-domain approval (parent confirmation in Q2).
- Capability environment snapshot is injectable; production reads process environment. Tests use private process state. Node `Buffer.from(base64)` semantics come from `pi-js-compat`.
- Kitty encoding corpus uses base64 ASCII; UTF16 lone-surrogate inputs are the existing Rust String boundary. Random IDs and native platform/tool environments are not exhaustive differential claims.

## Source discrepancies

- WebP initial `<30` check makes inner VP8L `<25` check unreachable; preserved and tested.
- Node ASCII decoding masks high bits even for invalid image headers; preserved, not tightened.

Mutation evidence: restored iTerm size, wrong filename, dropped GIF/WebP masks, wrong numeric formatter, forced-redraw ID clearing, unsuccessful tmux acceptance are killed by behavioral tests. See Q2 mutation ledger.
