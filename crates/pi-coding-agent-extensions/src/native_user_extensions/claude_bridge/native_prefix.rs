//! Preserve matching Claude records and carry only file attachments across rebuilds.
use super::{convert::content_text, stream::string};
use pi_js_compat::js_json::{JsString, JsValue};
use std::collections::HashMap;

#[derive(Clone)]
struct Unit {
    role: &'static str,
    key: JsValue,
    tools: bool,
    end: usize,
}

fn blocks(content: &JsValue) -> Vec<JsValue> {
    if let Some(text) = content.as_string() {
        return if text.is_empty() {
            Vec::new()
        } else {
            vec![JsValue::object([
                ("type", JsValue::String("text".into())),
                ("text", content.clone()),
            ])]
        };
    }
    content.as_array().cloned().unwrap_or_default()
}

fn assistant_unit(content: &[JsValue], end: usize) -> Unit {
    let key = content
        .iter()
        .filter_map(|block| {
            let kind = string(block, "type");
            if kind == "text" && !string(block, "text").is_empty() {
                Some(JsValue::Array(vec![
                    JsValue::String("text".into()),
                    block.get("text").cloned().unwrap_or(JsValue::Null),
                ]))
            } else if kind == "tool_use" {
                Some(JsValue::Array(vec![
                    JsValue::String("tool_use".into()),
                    block.get("id").cloned().unwrap_or(JsValue::Null),
                    block.get("name").cloned().unwrap_or(JsValue::Null),
                ]))
            } else {
                None
            }
        })
        .collect();
    Unit {
        role: "assistant",
        key: JsValue::Array(key),
        tools: content.iter().any(|b| string(b, "type") == "tool_use"),
        end,
    }
}

#[derive(Default)]
struct UserSide {
    results: Vec<JsValue>,
    texts: Vec<JsValue>,
    images: usize,
}
impl UserSide {
    fn add(&mut self, content: &JsValue) {
        for block in blocks(content) {
            match string(&block, "type").to_utf8().as_deref() {
                Ok("tool_result") => {
                    let content = block.get("content").unwrap_or(&JsValue::Null);
                    let images = blocks(content).iter().filter(|b| string(b, "type") == "image").count();
                    self.results.push(JsValue::Array(vec![
                        JsValue::String(string(&block, "tool_use_id")),
                        JsValue::String(content_text(content)),
                        JsValue::Bool(block.get("is_error").and_then(JsValue::as_bool).unwrap_or(false)),
                        JsValue::Number(images as f64),
                    ]));
                }
                Ok("text") if !string(&block, "text").is_empty() => {
                    self.texts.push(JsValue::String(string(&block, "text")))
                }
                Ok("image") => self.images += 1,
                _ => {}
            }
        }
    }
    fn key(&self) -> JsValue {
        let mut results = self.results.clone();
        results.sort_by_key(|r| {
            r.as_array()
                .and_then(|a| a.first())
                .and_then(JsValue::as_string)
                .cloned()
        });
        JsValue::object([
            ("results", JsValue::Array(results)),
            ("texts", JsValue::Array(self.texts.clone())),
            ("images", JsValue::Number(self.images as f64)),
        ])
    }
}

fn units(items: &[JsValue], records: bool) -> Vec<Unit> {
    let mut units: Vec<Unit> = Vec::new();
    let mut side: Option<UserSide> = None;
    let mut assistant: Option<(JsString, Vec<JsValue>)> = None;
    for (index, item) in items.iter().enumerate() {
        let message = if records {
            item.get("message").unwrap_or(&JsValue::Null)
        } else {
            item
        };
        let kind = string(item, if records { "type" } else { "role" });
        if kind == "assistant" {
            if records && string(message, "model") == "<synthetic>" {
                if let Some(last) = units.last_mut() {
                    last.end = index;
                }
                continue;
            }
            side = None;
            let id = string(message, "id");
            let continues = records && assistant.as_ref().is_some_and(|a| a.0 == id);
            let mut content = if continues {
                assistant.take().map(|a| a.1).unwrap_or_default()
            } else {
                Vec::new()
            };
            content.extend(blocks(message.get("content").unwrap_or(&JsValue::Null)));
            if continues {
                units.pop();
            }
            units.push(assistant_unit(&content, index));
            assistant = Some((id, content));
        } else {
            assistant = None;
            let content = if kind == "attachment" {
                item.get("attachment")
                    .filter(|a| string(a, "type") == "queued_command")
                    .and_then(|a| a.get("prompt"))
            } else if records && item.get("isMeta").and_then(JsValue::as_bool) == Some(true) {
                None
            } else {
                message.get("content")
            };
            if side.is_some() {
                units.pop();
            }
            let side = side.get_or_insert_with(UserSide::default);
            side.add(content.unwrap_or(&JsValue::Null));
            units.push(Unit {
                role: "user",
                key: side.key(),
                tools: false,
                end: index,
            });
        }
    }
    units
}

pub fn native_prefix(records: &[JsValue], messages: &[JsValue]) -> (Vec<JsValue>, usize) {
    let conversation: Vec<_> = records
        .iter()
        .filter(|record| {
            ["user", "assistant", "attachment"]
                .iter()
                .any(|kind| string(record, "type") == *kind)
                && record.get("isSidechain").and_then(JsValue::as_bool) != Some(true)
        })
        .cloned()
        .collect();
    let native = units(&conversation, true);
    let converted = units(messages, false);
    let mut matched = native
        .iter()
        .zip(&converted)
        .take_while(|(a, b)| a.role == b.role && a.key == b.key)
        .count();
    while matched > 0 && converted[matched - 1].tools {
        matched -= 1;
    }
    if matched == 0 {
        return (Vec::new(), 0);
    }
    (
        conversation[..=native[matched - 1].end].to_vec(),
        converted[matched - 1].end + 1,
    )
}

fn prompt_text(message: &JsValue) -> Option<JsString> {
    let content = message.get("content")?;
    if blocks(content).iter().any(|b| string(b, "type") == "tool_result") {
        return None;
    }
    let text = content_text(content);
    (!text.is_empty()).then_some(text)
}

pub fn tail_attachments(
    native: &[JsValue],
    kept: &[JsValue],
    messages: &[JsValue],
    covered: usize,
) -> (HashMap<usize, Vec<JsValue>>, Vec<JsString>) {
    let prompts: Vec<_> = messages
        .iter()
        .enumerate()
        .filter(|(_, m)| string(m, "role") == "user")
        .filter_map(|(i, m)| prompt_text(m).map(|t| (i, t)))
        .collect();
    let mut parents: HashMap<JsString, (usize, JsString)> = HashMap::new();
    let mut ordinal = 0;
    let mut placed: HashMap<usize, Vec<JsValue>> = HashMap::new();
    let mut skipped = Vec::new();
    for record in native {
        if string(record, "type") == "user" {
            if let Some(text) = record.get("message").and_then(prompt_text) {
                parents.insert(string(record, "uuid"), (ordinal, text));
                ordinal += 1;
            }
            continue;
        }
        if string(record, "type") != "attachment" {
            continue;
        }
        let Some(parent) = parents.get(&string(record, "parentUuid")).cloned() else {
            continue;
        };
        parents.insert(string(record, "uuid"), parent.clone());
        let Some(attachment) = record.get("attachment").filter(|a| string(a, "type") == "file") else {
            continue;
        };
        if kept.iter().any(|r| r.get("uuid") == record.get("uuid")) {
            continue;
        }
        let name = attachment
            .get("filename")
            .and_then(JsValue::as_string)
            .cloned()
            .unwrap_or_else(|| "file".into());
        match prompts.get(parent.0) {
            None => skipped.push(pi_js_compat::js_format!(
                "{}: prompt #{} is no longer in history",
                name,
                parent.0
            )),
            Some((_, text)) if text != &parent.1 => {
                skipped.push(pi_js_compat::js_format!("{}: prompt #{} changed", name, parent.0))
            }
            Some((index, _)) if *index >= covered => {
                placed.entry(*index - covered).or_default().push(attachment.clone())
            }
            _ => {}
        }
    }
    (placed, skipped)
}
