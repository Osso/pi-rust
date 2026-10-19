# tui/src/index.ts traceability

Source: `packages/tui/src/index.ts` (161 lines). Rust re-exports live in
`crates/pi-tui/src/lib.rs`. All paths below are relative to `pi_tui::`.

| TypeScript export | Rust re-export | Status |
| --- | --- | --- |
| `Marked`, `Token`, `Tokens` | `Marked` (facade over `components::markdown::lexer::lex`), `Token`, `Tokens` (`Vec<Token>`) | done |
| `AutocompleteItem`, `AutocompleteSuggestions`, `CombinedAutocompleteProvider`, `SlashCommand` | same names | done |
| `AutocompleteProvider` (type) | `AutocompleteProvider` trait (`autocomplete`) | done |
| `Box` | `Box` (`components::r#box::BoxComponent`) | done |
| `CancellableLoader` | `CancellableLoader` | done |
| `DynamicBorder` | `DynamicBorder` | done |
| `Editor`, `EditorOptions`, `EditorTheme` | same names | done |
| `HStack` | `HStack` | done |
| `Image`, `ImageOptions`, `ImageTheme` | same names | done |
| `Input` | `Input` | done |
| `Loader`, `LoaderIndicatorOptions` | same names | done |
| `DefaultTextStyle`, `Markdown`, `MarkdownOptions`, `MarkdownTheme` | same names | done |
| `ScrollView`, `ScrollViewOptions`, `ScrollViewScrollbar`, `ScrollViewScrollToOptions` | same names | done |
| `SelectItem`, `SelectList`, `SelectListLayoutOptions`, `SelectListTheme`, `SelectListTruncatePrimaryContext` | same names | done |
| `SettingItem`, `SettingsList`, `SettingsListTheme` | same names | done |
| `Spacer` | `Spacer` | done |
| `Text` | `Text` | done |
| `TruncatedText` | `TruncatedText` | done |
| `StackChild`, `StackEntry`, `StackEntryOptions`, `StackOptions`, `VStack` | `StackEntry`, `StackEntryOptions`, `StackOptions`, `VStack` | done (`StackChild` is the inline union; Rust uses `StackLayoutEntry`) |
| `EditorComponent` (type) | `EditorComponent` trait | done |
| `FuzzyMatch`, `fuzzyFilter`, `fuzzyMatch` | `FuzzyMatch`, `fuzzy_filter`, `fuzzy_match` | done |
| `getKeybindings`, `setKeybindings`, `TUI_KEYBINDINGS`, `KeybindingConflict`, `KeybindingDefinition`, `KeybindingsConfig`, `KeybindingsManager`, `KeyId` | same names (`tui_keybindings`) | done |
| `Keybinding`, `Keybindings`, `KeybindingDefinitions` (types) | `Keybinding` (`String` name), `Keybindings`/`KeybindingDefinitions` (ordered definition pairs) | done |
| `decodeKittyPrintable`, `isKeyRelease`, `isKeyRepeat`, `isKittyProtocolActive`, `KeyEventType`, `matchesKey`, `parseKey`, `setKittyProtocolActive` | `decode_kitty_printable`, `is_key_release`, `is_key_repeat`, `is_kitty_protocol_active`, `KeyEventType`, `matches_key`, `parse_key`, `set_kitty_protocol_active` | done |
| `Key` (const map) | `Key` namespace type: associated constants + modifier functions | done (`tests/index_exports.rs` differential over all 79 members) |
| `RenderLatexOptions`, `renderLatex` | `RenderLatexOptions`, `render_latex` | done |
| `RootCompositor`, `RootCompositorOptions`, `RootLayoutEntry` | same names | done |
| `RootFlowLayout`, `RootLayoutRect` | `tui::FlowRenderRegionLayout`, `tui::RenderRegionRect` | done (names differ) |
| `StdinBuffer`, `StdinBufferEventMap`, `StdinBufferOptions` | `StdinBuffer`, `StdinBufferEventMap` (alias), `StdinBufferOptions` | done |
| `isTerminalRawModeFailure`, `ProcessTerminal`, `Terminal`, `TerminalRawModeError` | same names (`is_terminal_raw_mode_failure`) | done |
| `parseOsc11BackgroundColor`, `parseTerminalColorSchemeReport`, `RgbColor`, `TerminalColorScheme` | `parse_osc11_background_color`, `parse_terminal_color_scheme_report`, `RgbColor`, `TerminalColorScheme` | done |
| `resetCapabilitiesCache` | `reset_capabilities_cache` | done |
| `allocateImageId`, `CellDimensions`, `calculateImageRows`, `deleteAllKittyImages`, `deleteKittyImage`, `detectCapabilities`, `encodeITerm2`, `encodeKitty`, `getCapabilities`, `getCellDimensions`, `getGifDimensions`, `getImageDimensions`, `getJpegDimensions`, `getPngDimensions`, `getWebpDimensions`, `hyperlink`, `ImageDimensions`, `ImageProtocol`, `ImageRenderOptions`, `imageFallback`, `renderImage`, `resetCapabilitiesCache`, `setCapabilities`, `setCellDimensions`, `TerminalCapabilities` | snake_case counterparts (`allocate_image_id`, `delete_all_kitty_images`, `delete_kitty_image`, `detect_capabilities`, `encode_iterm2`, `encode_kitty`, `get_capabilities`, `get_cell_dimensions`, `get_gif_dimensions`, `get_image_dimensions`, `get_jpeg_dimensions`, `get_png_dimensions`, `get_webp_dimensions`, `image_fallback`, `render_image`, `reset_capabilities_cache`, `set_capabilities`, `set_cell_dimensions`) | done |
| `Component`, `Container`, `CURSOR_MARKER`, `compositeTuiLine`, `FixedCell`, `FlowRenderRegion`, `FlowRenderRegionLayout`, `Focusable`, `isFocusable`, `isViewportTUI`, `OverlayAnchor`, `OverlayHandle`, `OverlayMargin`, `OverlayOptions`, `RenderRegion`, `RenderRegionRect`, `SizeValue`, `TUI`, `TuiInputListenerResult`, `TuiMode`, `TuiStopOptions`, `ViewportTUI` | same names (`composite_tui_line`, `is_focusable`, `is_viewport_tui`, `TuiExt as TUI`) | done |
| `OverlayUnfocusOptions` | `OverlayHandle::unfocus_to(Option<ComponentRef>)` | done (shape differs) |
| `TuiInputListener` (type) | `TuiInputListener` public alias | done |
| `TuiAltScreen`, `TuiAltScreenOptions` | same names | done |
| `TuiMainScreen`, `TuiMainScreenRenderState` | `TuiMainScreen`, `TuiMainScreenRenderState` (alias) | done |
| `getOsc8LinkAtColumn`, `sliceByColumn`, `stripTerminalSequences`, `truncateToWidth`, `visibleWidth`, `wrapTextWithAnsi` | snake_case counterparts | done |

## Deviations

- Rust re-exports use idiomatic snake_case for functions and a few renamed types
  (`BoxComponent as Box`, `MainScreenRenderState as TuiMainScreenRenderState`,
  `TuiExt as TUI`, `StdinBufferEvent as StdinBufferEventMap`).
- `Marked` is a namespace type with a static `lex`; TypeScript constructs an
  instance and calls `.lex`.
- `Key` modifier helpers use Rust naming (`super_key`, `ctrl_shift`, …) and
  return `String`; constants are associated constants (`Key::PAGE_UP`).
- `Keybinding`/`Keybindings`/`KeybindingDefinitions` are data aliases: Rust has
  no declaration merging, so the registry is the ordered definition list
  returned by `tui_keybindings`.
- `TuiInputListener` is a public `Rc<dyn Fn(&str) -> TuiInputListenerResult>`;
  `add_input_listener` returns an id, `remove_input_listener` takes it.
- `path_to_file_url` is a Rust-only package-root export used by the
  coding-agent renderer; the source keeps it private to `terminal-image.ts`.
