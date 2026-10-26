//! Used cc-session-io@0.4.0 repairToolPairing, preserving order and UTF-16 IDs.
use super::stream::string;
use indexmap::IndexSet;
use pi_js_compat::js_json::{JsString, JsValue};

fn synthetic(id: JsString) -> JsValue {
    JsValue::object([
        ("type", JsValue::String("tool_result".into())),
        ("tool_use_id", JsValue::String(id)),
        ("content", JsValue::String("[no tool result recorded]".into())),
        ("is_error", JsValue::Bool(true)),
    ])
}
fn flush_pending(output: &mut Vec<JsValue>, pending: &mut Option<IndexSet<JsString>>) {
    if let Some(ids) = pending.take().filter(|ids| !ids.is_empty()) {
        output.push(JsValue::object([
            ("role", JsValue::String("user".into())),
            ("content", JsValue::Array(ids.into_iter().map(synthetic).collect())),
        ]));
    }
}
fn repair_user(message: &JsValue, pending: Option<IndexSet<JsString>>, first: bool) -> Option<JsValue> {
    let content = message.get("content").unwrap_or(&JsValue::Null);
    let input = if let Some(blocks) = content.as_array() {
        blocks.clone()
    } else if let Some(text) = content.as_string().filter(|s| !s.is_empty()) {
        vec![JsValue::object([
            ("type", JsValue::String("text".into())),
            ("text", JsValue::String(text.clone())),
        ])]
    } else {
        Vec::new()
    };
    let mut provided = IndexSet::new();
    let kept = input
        .into_iter()
        .filter(|block| {
            if string(block, "type") != "tool_result" {
                return true;
            }
            if let Some(id) = block.get("tool_use_id").and_then(JsValue::as_string)
                && pending.as_ref().is_some_and(|ids| ids.contains(id))
            {
                provided.insert(id.clone());
                return true;
            }
            false
        })
        .collect::<Vec<_>>();
    let mut blocks = pending
        .into_iter()
        .flatten()
        .filter(|id| !provided.contains(id))
        .map(synthetic)
        .collect::<Vec<_>>();
    blocks.extend(kept);
    if blocks.is_empty() {
        return first.then(|| {
            JsValue::object([
                ("role", JsValue::String("user".into())),
                (
                    "content",
                    JsValue::Array(vec![JsValue::object([
                        ("type", JsValue::String("text".into())),
                        ("text", JsValue::String("[orphaned tool result removed]".into())),
                    ])]),
                ),
            ])
        });
    }
    let mut message = message.clone();
    if let JsValue::Object(fields) = &mut message {
        fields.insert("content".into(), JsValue::Array(blocks));
    }
    Some(message)
}
pub(super) fn repair_tool_pairing(messages: &[JsValue]) -> Vec<JsValue> {
    let mut output = Vec::new();
    let mut pending: Option<IndexSet<JsString>> = None;
    for message in messages {
        let content = message.get("content").unwrap_or(&JsValue::Null);
        if string(message, "role") == "assistant" {
            flush_pending(&mut output, &mut pending);
            let ids = content
                .as_array()
                .into_iter()
                .flatten()
                .filter(|b| string(b, "type") == "tool_use")
                .filter_map(|b| b.get("id").and_then(JsValue::as_string).cloned())
                .collect::<IndexSet<_>>();
            output.push(message.clone());
            pending = (!ids.is_empty()).then_some(ids);
            continue;
        }
        let has_results = content
            .as_array()
            .is_some_and(|blocks| blocks.iter().any(|b| string(b, "type") == "tool_result"));
        if pending.is_none() && !has_results {
            output.push(message.clone());
            continue;
        }
        if let Some(message) = repair_user(message, pending.take(), output.is_empty()) {
            output.push(message);
        }
    }
    flush_pending(&mut output, &mut pending);
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use pi_js_compat::js_json;
    #[test]
    fn claude_bridge_repair_matches_frozen_orphans_interleaving_and_utf16() {
        let fixture = js_json::parse_lossless(include_str!("fixtures/repair-oracle.json")).unwrap();
        for case in fixture.get("cases").unwrap().as_array().unwrap() {
            assert_eq!(
                JsValue::Array(repair_tool_pairing(case.get("input").unwrap().as_array().unwrap())),
                *case.get("expected").unwrap(),
                "{:?}",
                case.get("name")
            );
        }
    }
}
