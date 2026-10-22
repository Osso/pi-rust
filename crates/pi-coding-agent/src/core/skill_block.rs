use pi_js_compat::js_json::JsString;

#[derive(Debug)]
pub struct ParsedSkillBlock {
    pub name: JsString,
    pub location: JsString,
    pub content: JsString,
    pub user_message: Option<JsString>,
}

/// Parses one complete skill block without decoding source UTF-16 units.
pub fn parse_skill_block(text: impl Into<JsString>) -> Option<ParsedSkillBlock> {
    let text = text.into();
    let remaining = text.strip_prefix("<skill name=\"")?;
    let (name, remaining) = take_quoted_attribute(&remaining)?;
    let remaining = remaining.strip_prefix(" location=\"")?;
    let (location, remaining) = take_quoted_attribute(&remaining)?;
    let remaining = remaining.strip_prefix(">\n")?;
    let close = "\n</skill>";
    let mut start = 0;
    while let Some(content_end) = remaining.find_from(close, start) {
        let trailing = remaining.slice(content_end + close.len(), remaining.len());
        if let Some(user_message) = parse_user_message(&trailing) {
            return Some(ParsedSkillBlock {
                name,
                location,
                content: remaining.slice(0, content_end),
                user_message,
            });
        }
        start = content_end + close.len();
    }
    None
}

fn parse_user_message(trailing: &JsString) -> Option<Option<JsString>> {
    if trailing.is_empty() {
        return Some(None);
    }
    let suffix = trailing.strip_prefix("\n\n")?;
    if suffix.is_empty() {
        return None;
    }
    let message = suffix.trim();
    Some((!message.is_empty()).then_some(message))
}

fn take_quoted_attribute(value: &JsString) -> Option<(JsString, JsString)> {
    let end = value.find("\"")?;
    (end > 0).then(|| (value.slice(0, end), value.slice(end + 1, value.len())))
}

#[cfg(test)]
mod lossless_tests {
    use super::*;
    use pi_js_compat::js_json::JsString;
    #[test]
    fn skill_block_keeps_content_and_user_message_raw() {
        let text = JsString::from("<skill name=\"tool\" location=\"/tmp/tool\">\n")
            + JsString::from_utf16(vec![0xd800])
            + "\n</skill>\n\n "
            + JsString::from_utf16(vec![0xd801, 0xd83d, 0xde00])
            + " ";
        let parsed = parse_skill_block(&text).unwrap();
        assert_eq!(parsed.content.units(), &[0xd800]);
        assert_eq!(parsed.user_message.unwrap().units(), &[0xd801, 0xd83d, 0xde00]);
    }
}
