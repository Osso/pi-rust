use indexmap::IndexMap;
use pi_js_compat::js_json::JsString;

const GOAL_STATUS_KEY: &str = "goal";

/// Replace control whitespace with spaces, collapse spaces, then apply JS trim.
pub fn sanitize_footer_status_text(text: impl Into<JsString>) -> JsString {
    let text = text.into();
    let mut collapsed = JsString::default();
    let mut previous_was_space = false;
    for unit in text.units().iter().copied() {
        let unit = if matches!(unit, 9 | 10 | 13) { 32 } else { unit };
        if unit == 32 && previous_was_space {
            continue;
        }
        previous_was_space = unit == 32;
        collapsed.push_unit(unit);
    }
    collapsed.trim()
}

/// Source locale-sorted statuses, with other statuses joined before the goal.
pub fn format_extension_status_text_lines(statuses: &IndexMap<JsString, JsString>) -> Vec<JsString> {
    let mut lines = Vec::new();
    let mut other_statuses = statuses
        .iter()
        .filter(|(key, _)| *key != GOAL_STATUS_KEY)
        .collect::<Vec<_>>();
    other_statuses
        .sort_by(|(left, _), (right, _)| pi_js_compat::collator::locale_compare_utf16(left.units(), right.units()));
    if !other_statuses.is_empty() {
        lines.push(JsString::join(
            other_statuses
                .iter()
                .map(|(_, text)| sanitize_footer_status_text(*text)),
            " ",
        ));
    }
    if let Some(goal) = statuses.get(GOAL_STATUS_KEY) {
        lines.push(sanitize_footer_status_text(goal));
    }
    lines
}
