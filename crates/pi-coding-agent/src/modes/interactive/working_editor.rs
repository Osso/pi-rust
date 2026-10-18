//! Ported from `coding-agent/src/modes/interactive/working-editor.ts`.

use pi_tui::EditorComponent;
use pi_tui::LoaderIndicatorOptions;
use pi_tui::utils::visible_width;

fn working_indicator_fits_prompt(indicator: Option<&LoaderIndicatorOptions>) -> bool {
    let Some(frames) = indicator.and_then(|indicator| indicator.frames.as_ref()) else {
        return true;
    };
    if frames.is_empty() {
        return true;
    }
    frames.iter().all(|frame| visible_width(frame) == 1)
}

/// `supportsWorkingPromptAnimation`: whether the editor exposes the working
/// prompt surface and the indicator frames fit in one cell.
pub fn supports_working_prompt_animation(
    editor: &dyn EditorComponent,
    indicator: Option<&LoaderIndicatorOptions>,
) -> bool {
    editor.supports_working_prompt() && working_indicator_fits_prompt(indicator)
}

/// `syncWorkingEditor`.
pub fn sync_working_editor(editor: &mut dyn EditorComponent, working: bool, indicator: Option<LoaderIndicatorOptions>) {
    if editor.supports_working_prompt() && !working_indicator_fits_prompt(indicator.as_ref()) {
        editor.set_working(false);
        return;
    }
    editor.set_working_indicator(indicator);
    editor.set_working(working);
}

/// `positionWorkingEditor`.
pub fn position_working_editor(editor: &mut dyn EditorComponent, row: i64, col: i64) {
    editor.set_screen_origin(row, col);
}

/// `clearWorkingEditorPosition`.
pub fn clear_working_editor_position(editor: &mut dyn EditorComponent) {
    editor.clear_screen_origin();
}

/// `clearWorkingEditor`.
pub fn clear_working_editor(editor: &mut dyn EditorComponent) {
    editor.set_working(false);
    editor.clear_screen_origin();
}
