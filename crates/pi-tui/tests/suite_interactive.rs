// Process-local integration fixtures; process-sensitive fixtures keep standalone targets.

#[path = "differential_editor.rs"]
mod differential_editor;
#[path = "differential_markdown.rs"]
mod differential_markdown;
#[path = "differential_tui.rs"]
mod differential_tui;
#[path = "editor_abort.rs"]
mod editor_abort;
#[path = "editor_autocomplete.rs"]
mod editor_autocomplete;
#[path = "editor_editing.rs"]
mod editor_editing;
#[path = "editor_history.rs"]
mod editor_history;
#[path = "editor_paste.rs"]
mod editor_paste;
#[path = "editor_rendering.rs"]
mod editor_rendering;
#[path = "editor_sticky.rs"]
mod editor_sticky;
#[path = "editor_word_wrap.rs"]
mod editor_word_wrap;
#[path = "editor_working.rs"]
mod editor_working;
#[path = "markdown.rs"]
mod markdown;
#[path = "markdown_isimageline_regression.rs"]
mod markdown_isimageline_regression;
#[path = "raw_editor.rs"]
mod raw_editor;
#[path = "raw_markdown.rs"]
mod raw_markdown;
#[path = "render_region.rs"]
mod render_region;
#[path = "render_region_safety.rs"]
mod render_region_safety;
#[path = "tui_cell_size_input.rs"]
mod tui_cell_size_input;
#[path = "tui_focus.rs"]
mod tui_focus;
#[path = "tui_overlay_style_leak.rs"]
mod tui_overlay_style_leak;
#[path = "tui_render.rs"]
mod tui_render;
#[path = "tui_shrink.rs"]
mod tui_shrink;
