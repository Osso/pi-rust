# tui/src/index.ts traceability

Source: `packages/tui/src/index.ts` (161 lines). Rust re-exports live in
`crates/pi-tui/src/lib.rs`. All paths below are relative to `pi_tui::`.

| TypeScript export | Rust re-export | Status |
| --- | --- | --- |
| `Marked`, `Token`, `Tokens` | — | pending: marked package wrapper not ported (lexer exposes `components::markdown::lexer::lex` and `tokens::Token`) |
| `AutocompleteItem`, `AutocompleteSuggestions`, `CombinedAutocompleteProvider`, `SlashCommand` | same names | done |
| `AutocompleteProvider` (type) | — | pending: Rust uses concrete `CombinedAutocompleteProvider` |
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
| `Keybinding`, `Keybindings` (types) | — | pending: Rust uses `KeybindingDefinition`/`KeybindingsManager` directly |
| `decodeKittyPrintable`, `isKeyRelease`, `isKeyRepeat`, `isKittyProtocolActive`, `KeyEventType`, `matchesKey`, `parseKey`, `setKittyProtocolActive` | `decode_kitty_printable`, `is_key_release`, `is_key_repeat`, `is_kitty_protocol_active`, `KeyEventType`, `matches_key`, `parse_key`, `set_kitty_protocol_active` | done |
| `Key` (const map) | — | pending: Rust callers pass key id strings |
| `RenderLatexOptions`, `renderLatex` | `RenderLatexOptions`, `render_latex` | done |
| `RootCompositor`, `RootCompositorOptions`, `RootLayoutEntry` | same names | done |
| `RootFlowLayout`, `RootLayoutRect` | `tui::FlowRenderRegionLayout`, `tui::RenderRegionRect` | done (names differ) |
| `StdinBuffer`, `StdinBufferEventMap`, `StdinBufferOptions` | `StdinBuffer`, `StdinBufferEventMap` (alias), `StdinBufferOptions` | done |
| `isTerminalRawModeFailure`, `ProcessTerminal`, `Terminal`, `TerminalRawModeError` | same names (`is_terminal_raw_mode_failure`) | done |
| `parseOsc11BackgroundColor`, `parseTerminalColorSchemeReport`, `RgbColor`, `TerminalColorScheme` | `parse_osc11_background_color`, `parse_terminal_color_scheme_report`, `RgbColor`, `TerminalColorScheme` | done |
| `allocateImageId`, `CellDimensions`, `calculateImageRows`, `deleteAllKittyImages`, `deleteKittyImage`, `detectCapabilities`, `encodeITerm2`, `encodeKitty`, `getCapabilities`, `getCellDimensions`, `getGifDimensions`, `getImageDimensions`, `getJpegDimensions`, `getPngDimensions`, `getWebpDimensions`, `hyperlink`, `ImageDimensions`, `ImageProtocol`, `ImageRenderOptions`, `imageFallback`, `renderImage`, `resetCapabilitiesCache`, `setCapabilities`, `setCellDimensions`, `TerminalCapabilities` | snake_case counterparts (`allocate_image_id`, `delete_all_kitty_images`, `delete_kitty_image`, `detect_capabilities`, `encode_iterm2`, `encode_kitty`, `get_capabilities`, `get_cell_dimensions`, `get_gif_dimensions`, `get_image_dimensions`, `get_jpeg_dimensions`, `get_png_dimensions`, `get_webp_dimensions`, `image_fallback`, `render_image`, `reset_capabilities_cache`, `set_capabilities`, `set_cell_dimensions`) | done |
| `Component`, `Container`, `CURSOR_MARKER`, `compositeTuiLine`, `FixedCell`, `FlowRenderRegion`, `FlowRenderRegionLayout`, `Focusable`, `isFocusable`, `isViewportTUI`, `OverlayAnchor`, `OverlayHandle`, `OverlayMargin`, `OverlayOptions`, `RenderRegion`, `RenderRegionRect`, `SizeValue`, `TUI`, `TuiInputListenerResult`, `TuiMode`, `TuiStopOptions`, `ViewportTUI` | same names (`composite_tui_line`, `is_focusable`, `is_viewport_tui`, `TuiExt as TUI`) | done |
| `OverlayUnfocusOptions` | `OverlayHandle::unfocus_to(Option<ComponentRef>)` | done (shape differs) |
| `TuiInputListener` (type) | — | pending: listener alias is crate-private; `add_input_listener` accepts the closure type |
| `TuiAltScreen`, `TuiAltScreenOptions` | same names | done |
| `TuiMainScreen`, `TuiMainScreenRenderState` | `TuiMainScreen`, `TuiMainScreenRenderState` (alias) | done |
| `getOsc8LinkAtColumn`, `sliceByColumn`, `stripTerminalSequences`, `truncateToWidth`, `visibleWidth`, `wrapTextWithAnsi` | snake_case counterparts | done |

## Deviations

- Rust re-exports use idiomatic snake_case for functions and a few renamed types
  (`BoxComponent as Box`, `MainScreenRenderState as TuiMainScreenRenderState`,
  `TuiExt as TUI`, `StdinBufferEvent as StdinBufferEventMap`).
- Pending exports have no
  Rust counterpart yet (`Marked`/`Token`/`Tokens`, `Key`, provider/interface
  aliases).
