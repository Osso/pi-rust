//! Rust port of Pi's terminal UI library.

mod mermaid;
pub mod text_units;
mod unicode_tables;

pub mod autocomplete;
pub mod components;
pub mod editor_component;
pub mod fuzzy;
pub mod keybindings;
pub mod keys;
pub mod kill_ring;
pub mod native_modifiers;
pub mod root_compositor;
pub mod stdin_buffer;
pub mod terminal;
pub mod terminal_colors;
pub mod terminal_image;
pub mod tui;
pub mod undo_stack;
pub mod utils;
pub mod word_navigation;

// Package-root re-exports matching `packages/tui/src/index.ts`.
pub use autocomplete::{
    AutocompleteItem, AutocompleteProvider, AutocompleteSuggestions, CombinedAutocompleteProvider, SlashCommand,
};
pub use components::r#box::BoxComponent as Box;
pub use components::cancellable_loader::CancellableLoader;
pub use components::dynamic_border::DynamicBorder;
pub use components::editor::{Editor, EditorOptions, EditorTheme};
pub use components::image::{Image, ImageOptions, ImageTheme};
pub use components::input::Input;
pub use components::loader::{Loader, LoaderIndicatorOptions};
pub use components::markdown::{DefaultTextStyle, Markdown, MarkdownOptions, MarkdownTheme};
pub use components::select_list::{
    SelectItem, SelectList, SelectListLayoutOptions, SelectListTheme, SelectListTruncatePrimaryContext,
};
pub use components::settings_list::{SettingItem, SettingsList, SettingsListTheme};
pub use components::spacer::Spacer;
pub use components::text::Text;
pub use components::truncated_text::TruncatedText;
pub use editor_component::EditorComponent;
pub use fuzzy::{FuzzyMatch, fuzzy_filter, fuzzy_match};
pub use keybindings::{
    KeyId, Keybinding, KeybindingConflict, KeybindingDefinition, KeybindingDefinitions, Keybindings, KeybindingsConfig,
    KeybindingsManager, get_keybindings, set_keybindings, tui_keybindings,
};
pub use keys::{
    Key, KeyEventType, decode_kitty_printable, is_key_release, is_key_repeat, is_kitty_protocol_active, matches_key,
    parse_key, set_kitty_protocol_active,
};
pub use root_compositor::{RootCompositor, RootCompositorOptions, RootLayoutEntry};
pub use stdin_buffer::{StdinBuffer, StdinBufferEvent as StdinBufferEventMap, StdinBufferOptions};
pub use terminal::{ProcessTerminal, Terminal, TerminalRawModeError, is_terminal_raw_mode_failure};
pub use terminal_colors::{
    RgbColor, TerminalColorScheme, parse_osc11_background_color, parse_terminal_color_scheme_report,
};
pub use terminal_image::{
    CellDimensions, ImageDimensions, ImageProtocol, ImageRenderOptions, TerminalCapabilities, allocate_image_id,
    calculate_image_rows, delete_all_kitty_images, delete_kitty_image, detect_capabilities, encode_iterm2,
    encode_kitty, get_capabilities, get_cell_dimensions, get_gif_dimensions, get_image_dimensions, get_jpeg_dimensions,
    get_png_dimensions, get_webp_dimensions, hyperlink, image_fallback, render_image, reset_capabilities_cache,
    set_capabilities, set_cell_dimensions,
};
pub use tui::{
    CURSOR_MARKER, Component, Container, FixedCell, FlowRenderRegion, FlowRenderRegionLayout, Focusable, OverlayAnchor,
    OverlayHandle, OverlayMargin, OverlayOptions, RenderRegion, RenderRegionRect, SizeValue, TuiRenderer as TUI,
    is_focusable,
};
pub use tui::{FlowRenderRegionLayout as RootFlowLayout, RenderRegionRect as RootLayoutRect};
pub use utils::{slice_by_column, truncate_to_width, visible_width, wrap_text_with_ansi};
