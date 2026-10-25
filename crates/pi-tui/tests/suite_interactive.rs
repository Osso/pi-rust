// Process-local integration fixtures; process-sensitive fixtures keep standalone targets.

#[path = "support/components.rs"]
mod shared_components;
#[path = "support/editor_host.rs"]
mod shared_editor_host;
#[path = "support/oracle.rs"]
mod shared_oracle;
#[path = "support/virtual_terminal.rs"]
mod shared_virtual_terminal;

#[path = "differential_editor.rs"]
pub mod differential_editor;
#[path = "differential_tui.rs"]
pub mod differential_tui;
#[path = "editor_abort.rs"]
pub mod editor_abort;
#[path = "editor_autocomplete.rs"]
pub mod editor_autocomplete;
#[path = "editor_editing.rs"]
pub mod editor_editing;
#[path = "editor_history.rs"]
pub mod editor_history;
#[path = "editor_paste.rs"]
pub mod editor_paste;
#[path = "editor_rendering.rs"]
pub mod editor_rendering;
#[path = "editor_sticky.rs"]
pub mod editor_sticky;
#[path = "editor_word_wrap.rs"]
pub mod editor_word_wrap;
#[path = "editor_working.rs"]
pub mod editor_working;
#[path = "markdown_isimageline_regression.rs"]
pub mod markdown_isimageline_regression;
#[path = "raw_editor.rs"]
pub mod raw_editor;
#[path = "raw_markdown.rs"]
pub mod raw_markdown;
#[path = "render_region.rs"]
pub mod render_region;
#[path = "render_region_safety.rs"]
pub mod render_region_safety;
#[path = "tui_focus.rs"]
pub mod tui_focus;
#[path = "tui_overlay_style_leak.rs"]
pub mod tui_overlay_style_leak;
#[path = "tui_shrink.rs"]
pub mod tui_shrink;
