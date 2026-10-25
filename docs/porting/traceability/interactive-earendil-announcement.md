# earendil-announcement.ts

Source: `coding-agent/src/modes/interactive/components/earendil-announcement.ts`
Rust: `crates/pi-coding-agent-ui/src/modes/interactive/components/earendil_announcement.rs`

| Symbol / branch | Rust item | Proof |
|---|---|---|
| module-level image cache (`attemptedImageLoad`) | `OnceLock<Option<String>>` | `interactive_components::earendil_announcement_renders_text_and_borders_without_the_bundled_image` |
| image read failure → `undefined` | `std::fs::read(..).ok()` | same test |
| accent borders, bold title, muted text, `mdLink` URL, spacers | `EarendilAnnouncementComponent::new` | same test |
| image branch (`Image`, `maxWidthCells: 56`) | `Image::new` + `ImageOptions` | gap: the test binary has no bundled `assets/clankolas.png`; the branch needs the packaged asset |
| `getBundledInteractiveAssetPath` | `config::get_bundled_interactive_asset_path` | same test (path resolution) |

Gap: the image-present branch is not executed in tests because the asset ships next to the packaged executable.

## Q18 native UTF-16

Native style callbacks and render output; assistant text/thinking/labels/errors no longer encode before Markdown/Text. Behavioral cases in `raw_error_consumer_components` written **unexecuted** pending coding-agent compilation. Earendil image filesystem load omission follows frozen source lines 14-22; no change to that source behavior.
