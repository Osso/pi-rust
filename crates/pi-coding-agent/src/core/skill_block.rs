#[derive(Debug)]
pub struct ParsedSkillBlock {
    pub name: String,
    pub location: String,
    pub content: String,
    pub user_message: Option<String>,
}

/// Parses one complete skill block from user-message text.
pub fn parse_skill_block(text: &str) -> Option<ParsedSkillBlock> {
    let remaining = text.strip_prefix("<skill name=\"")?;
    let (name, remaining) = take_quoted_attribute(remaining)?;
    let remaining = remaining.strip_prefix(" location=\"")?;
    let (location, remaining) = take_quoted_attribute(remaining)?;
    let remaining = remaining.strip_prefix(">\n")?;
    let close = "\n</skill>";
    for (content_end, _) in remaining.match_indices(close) {
        let trailing = &remaining[content_end + close.len()..];
        let Some(user_message) = parse_user_message(trailing) else {
            continue;
        };

        return Some(ParsedSkillBlock {
            name: name.to_owned(),
            location: location.to_owned(),
            content: remaining[..content_end].to_owned(),
            user_message,
        });
    }

    None
}

fn parse_user_message(trailing: &str) -> Option<Option<String>> {
    if trailing.is_empty() {
        return Some(None);
    }

    let suffix = trailing.strip_prefix("\n\n")?;
    if suffix.is_empty() {
        return None;
    }

    let message = trim_javascript(suffix);
    Some((!message.is_empty()).then(|| message.to_owned()))
}

fn take_quoted_attribute(value: &str) -> Option<(&str, &str)> {
    let end = value.find('"')?;
    (end > 0).then(|| (&value[..end], &value[end + 1..]))
}

fn trim_javascript(value: &str) -> &str {
    value.trim_matches(is_javascript_trim_character)
}

fn is_javascript_trim_character(character: char) -> bool {
    matches!(
        character,
        '\u{0009}'..='\u{000D}'
            | '\u{0020}'
            | '\u{00A0}'
            | '\u{1680}'
            | '\u{2000}'..='\u{200A}'
            | '\u{2028}'
            | '\u{2029}'
            | '\u{202F}'
            | '\u{205F}'
            | '\u{3000}'
            | '\u{FEFF}'
    )
}
