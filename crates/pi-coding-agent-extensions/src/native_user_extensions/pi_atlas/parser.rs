//! Session JSONL parsing, including stateful skill attribution.
use super::data::{self, Dictionary, ModelUsage, Session, SkillUsage, text};
use super::format::{basename, language_from_path};
use pi_js_compat::date::{date_parse_utf16, to_iso_string_value};
use pi_js_compat::js_json::{self, JsString, JsValue};

#[derive(Default)]
struct SkillState {
    active: Option<(JsString, bool)>,
}

pub struct ParsedLog {
    pub session: Option<Session>,
    pub corrupt_lines: usize,
    pub diagnostics: Vec<JsString>,
}

pub fn parse_text(text: &JsString) -> Result<Option<JsValue>, JsString> {
    parse_log(text).session.as_ref().map(data::session_value).transpose()
}

pub fn parse_log(content: &JsString) -> ParsedLog {
    let mut output = ParsedLog {
        session: None,
        corrupt_lines: 0,
        diagnostics: Vec::new(),
    };
    let mut state = SkillState::default();
    for line in content
        .split('\n')
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
    {
        let parsed = js_json::parse_lossless(line)
            .map_err(|error| pi_js_compat::js_format!("SyntaxError: {}", error.into_message()))
            .and_then(|entry| parse_entry(&entry, &mut state, &mut output.diagnostics));
        match parsed {
            Ok(Some(session)) => match &mut output.session {
                Some(base) => data::merge_session(base, session),
                None => output.session = Some(session),
            },
            Ok(None) => {}
            Err(error) => {
                output.corrupt_lines += 1;
                output.diagnostics.push(error);
            }
        }
    }
    output
}

fn iso_date(value: &JsValue) -> Result<JsString, JsString> {
    let millis = match value {
        JsValue::String(value) => date_parse_utf16(value.units()),
        JsValue::Number(value) => *value,
        JsValue::Null => 0.0,
        _ => f64::NAN,
    };
    to_iso_string_value(millis)
        .map(Into::into)
        .map_err(|error| pi_js_compat::js_format!("RangeError: {}", error.to_string()))
}

fn parse_entry(
    entry: &JsValue,
    state: &mut SkillState,
    warnings: &mut Vec<JsString>,
) -> Result<Option<Session>, JsString> {
    if !entry.is_object() {
        return Ok(None);
    }
    let kind = text(entry, "type");
    if ![
        "session",
        "message",
        "model_change",
        "thinking_level_change",
        "compaction",
    ]
    .iter()
    .any(|candidate| kind == *candidate)
    {
        return Ok(None);
    }
    let mut session = Session {
        timestamp: iso_date(&entry["timestamp"])?,
        ..Session::default()
    };
    if kind == "session" {
        session.session_id = text(entry, "id");
        session.cwd = text(entry, "cwd");
        session.project = basename(&session.cwd);
    } else if kind == "model_change" {
        session.model_changes = 1.0;
    } else if kind == "thinking_level_change" {
        session.thinking_level_count.insert(text(entry, "thinkingLevel"), 1.0);
    } else if kind == "compaction" {
        session.compaction_count = 1.0;
        session.compacted_tokens = data::number(entry, "tokensBefore");
    } else {
        parse_message(&entry["message"], &mut session, state, warnings)?;
    }
    Ok(Some(session))
}

fn parse_message(
    message: &JsValue,
    session: &mut Session,
    state: &mut SkillState,
    warnings: &mut Vec<JsString>,
) -> Result<(), JsString> {
    let role = text(message, "role");
    let timestamp = if role == "user" {
        JsValue::Number(data::number(message, "timestamp") * 1000.0)
    } else {
        message["timestamp"].clone()
    };
    // The source creates an intermediate aggregate even though its timestamp is discarded.
    iso_date(&timestamp)?;
    if role == "user" {
        session.user_msgs = 1.0;
        state.active = explicit_skill(&text(message, "content")).map(|name| (name, false));
    } else if role == "toolResult" {
        session.tool_results = 1.0;
    } else if role == "assistant" {
        parse_assistant(message, session, state, warnings)?;
    }
    Ok(())
}

fn explicit_skill(content: &JsString) -> Option<JsString> {
    let mut found = None;
    let mut offset = 0;
    while let Some(start) = content.find_from("<skill", offset) {
        let tail = content.slice(start + 6, content.len());
        let trimmed = tail.trim_start();
        if trimmed.len() < tail.len() && trimmed.starts_with("name=\"") {
            let name = trimmed.slice(6, trimmed.len());
            if let Some(end) = name.find("\"")
                && end > 0
            {
                found = Some(name.slice(0, end));
            }
        }
        offset = start + 6;
    }
    found
}

fn tool_arguments(block: &JsValue, warnings: &mut Vec<JsString>) -> Option<JsValue> {
    match block.get("arguments")? {
        JsValue::String(raw) => match js_json::parse_lossless(raw.clone()) {
            Ok(value) => Some(value),
            Err(_) => {
                warnings.push(pi_js_compat::js_format!(
                    "Failed to parse JSON tool arguments: {}",
                    raw.slice(0, 200)
                ));
                None
            }
        },
        value if value.is_object() || value.is_array() => Some(value.clone()),
        _ => None,
    }
}

fn parse_assistant(
    message: &JsValue,
    session: &mut Session,
    state: &mut SkillState,
    warnings: &mut Vec<JsString>,
) -> Result<(), JsString> {
    let blocks = message
        .get("content")
        .and_then(JsValue::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default();
    discover_skill(blocks, state, warnings);
    if let Some((name, counted)) = &mut state.active
        && let Some(usage) = message.get("usage")
    {
        session.skills.insert(
            name.clone(),
            SkillUsage {
                usage: usage.clone(),
                calls: if *counted { 0.0 } else { 1.0 },
            },
        );
        *counted = true;
    }
    let name = text(message, "model");
    if name.is_empty() {
        return Ok(());
    }
    let provider = text(message, "provider");
    let mut model = ModelUsage {
        provider: provider.clone(),
        api: message.get("api").and_then(JsValue::as_string).cloned(),
        calls: 1.0,
        usage: message["usage"].clone(),
        asst_msgs: 1.0,
        tools: Dictionary::new(),
        languages: Dictionary::new(),
    };
    for block in blocks.iter().filter(|block| text(block, "type") == "toolCall") {
        let tool = text(block, "name");
        *model.tools.entry(sanitize_tool(&tool)).or_default() += 1.0;
        if (tool == "edit" || tool == "write")
            && let Some(arguments) = tool_arguments(block, warnings)
        {
            attribute_language(&mut model, &tool, &arguments);
        }
    }
    session.models.entry(provider).or_default().insert(name, model);
    Ok(())
}

fn discover_skill(blocks: &[JsValue], state: &mut SkillState, warnings: &mut Vec<JsString>) {
    if state.active.is_some() {
        return;
    }
    for block in blocks
        .iter()
        .filter(|block| text(block, "type") == "toolCall" && text(block, "name") == "read")
    {
        let Some(arguments) = tool_arguments(block, warnings) else {
            continue;
        };
        let path = text(&arguments, "path");
        if path.ends_with("/SKILL.md") {
            let segments: Vec<_> = path.split('/').collect();
            if let Some(name) = segments
                .get(segments.len().saturating_sub(2))
                .filter(|name| !name.is_empty())
            {
                state.active = Some((name.clone(), false));
            }
            break;
        }
    }
}

fn sanitize_tool(name: &JsString) -> JsString {
    JsString::from_utf16(
        name.units()
            .iter()
            .copied()
            .filter(|unit| !matches!(unit, 0..=31 | 127 | 0x200b..=0x200f | 0x2028..=0x2029 | 0xfeff))
            .collect(),
    )
}

fn newlines(value: &JsValue, key: &str) -> f64 {
    text(value, key).units().iter().filter(|unit| **unit == 10).count() as f64
}

fn attribute_language(model: &mut ModelUsage, tool: &JsString, arguments: &JsValue) {
    let path = text(arguments, "path");
    if path.is_empty() {
        return;
    }
    let language = model.languages.entry(language_from_path(&path)).or_default();
    if tool == "write" {
        language.lines += 1.0 + newlines(arguments, "content");
    } else if let Some(edits) = arguments.get("edits").and_then(JsValue::as_array) {
        language.lines += edits
            .iter()
            .map(|edit| newlines(edit, "newText") + newlines(edit, "oldText") + 1.0)
            .sum::<f64>();
        language.edits += edits.len() as f64;
    } else {
        language.lines += 1.0;
        language.edits += 1.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pi_js_compat::js_json;
    #[test]
    fn corrupt_lines_report_node_error_class_and_retain_valid_entries() {
        let parsed =
            parse_log(&"bad\n{\"type\":\"session\",\"id\":\"valid\",\"timestamp\":\"2026-10-25T10:00:00Z\"}\n".into());
        assert_eq!(parsed.corrupt_lines, 1);
        assert!(parsed.diagnostics[0].starts_with("SyntaxError: "));
        assert_eq!(parsed.session.expect("valid session").session_id, "valid");
    }

    #[test]
    fn matches_frozen_parser_fixtures_including_utf16() -> Result<(), JsString> {
        let fixture =
            js_json::parse_lossless(include_str!("fixtures/domain.json")).map_err(|error| error.into_message())?;
        for log in fixture["logs"].as_array().expect("logs") {
            let actual = parse_text(log["input"].as_string().expect("input"))?.unwrap_or(JsValue::Null);
            assert_eq!(actual, log["expected"]);
        }
        Ok(())
    }

    #[test]
    fn aggregates_session_messages_skills_and_language_edits() -> Result<(), JsString> {
        let input = concat!(
            "{\"type\":\"session\",\"id\":\"s1\",\"timestamp\":\"2026-10-25T10:00:00.000Z\",\"cwd\":\"/work/demo\"}\n",
            "{\"type\":\"message\",\"timestamp\":\"2026-10-25T10:00:01.000Z\",\"message\":{\"role\":\"user\",\"timestamp\":1792922401000,\"content\":\"<skill name=\\\"rust\\\">go\"}}\n",
            "{\"type\":\"message\",\"timestamp\":\"2026-10-25T10:00:02.000Z\",\"message\":{\"role\":\"assistant\",\"timestamp\":1792922402000,\"model\":\"test\",\"provider\":\"faux\",\"api\":\"faux\",\"usage\":{\"input\":10,\"output\":5,\"cacheRead\":0,\"cacheWrite\":0,\"totalTokens\":15,\"cost\":{\"input\":0.1,\"output\":0.2,\"cacheRead\":0,\"cacheWrite\":0,\"total\":0.3}},\"content\":[{\"type\":\"toolCall\",\"name\":\"edit\",\"arguments\":{\"path\":\"src/lib.rs\",\"edits\":[{\"oldText\":\"a\\n\",\"newText\":\"b\\nc\\n\"}]}}]}}\n"
        );
        let result = parse_text(&input.into())?.ok_or_else(|| JsString::from("missing session"))?;
        assert_eq!(result["sessionId"], JsValue::String("s1".into()));
        assert_eq!(result["project"], JsValue::String("demo".into()));
        assert_eq!(result["userMsgs"].as_f64(), Some(1.0));
        assert_eq!(
            result["models"]["faux"]["test"]["languages"]["Rust"]["lines"].as_f64(),
            Some(4.0)
        );
        assert_eq!(result["skills"]["rust"]["calls"].as_f64(), Some(1.0));
        let raw = js_json::stringify_lossless(&result);
        assert_eq!(js_json::parse_lossless(raw).map_err(|e| e.into_message())?, result);
        Ok(())
    }
}
