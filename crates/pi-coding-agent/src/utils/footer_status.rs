use std::collections::BTreeMap;

const GOAL_STATUS_KEY: &str = "goal";

/// Replace control whitespace with spaces, collapse runs of spaces, and trim.
pub fn sanitize_footer_status_text(text: &str) -> String {
    let flattened = text.replace(['\r', '\n', '\t'], " ");
    let mut collapsed = String::with_capacity(flattened.len());
    let mut previous_was_space = false;
    for character in flattened.chars() {
        if character == ' ' {
            if !previous_was_space {
                collapsed.push(character);
            }
            previous_was_space = true;
            continue;
        }
        previous_was_space = false;
        collapsed.push(character);
    }
    collapsed.trim().to_owned()
}

/// Format extension status lines: other statuses joined first, then the goal.
///
/// The source sorts keys with `localeCompare`; the Rust boundary uses ordinary
/// string ordering, which matches for ASCII keys.
pub fn format_extension_status_text_lines(statuses: &BTreeMap<String, String>) -> Vec<String> {
    let mut lines = Vec::new();

    let other_status_line = statuses
        .iter()
        .filter(|(key, _)| key.as_str() != GOAL_STATUS_KEY)
        .map(|(_, text)| sanitize_footer_status_text(text))
        .collect::<Vec<_>>()
        .join(" ");
    if !other_status_line.is_empty() {
        lines.push(other_status_line);
    }

    if let Some(goal) = statuses.get(GOAL_STATUS_KEY) {
        lines.push(sanitize_footer_status_text(goal));
    }

    lines
}
