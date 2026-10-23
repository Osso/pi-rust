use pi_ai::types::{AssistantContent, AssistantMessage, Message, TextOrImageContent, UserContent};
use pi_js_compat::{
    js_json as json,
    js_json::{JsString, JsValue, from_value, to_value},
};

#[test]
fn typed_message_and_content_unions_do_not_apply_json_stringify_number_projection() {
    let mut value = to_value(&AssistantMessage::empty("faux", "fixture", "fixture", 0.0)).unwrap();
    value["timestamp"] = JsValue::Number(f64::INFINITY);
    value["usage"]["input"] = JsValue::Number(f64::NAN);
    let call = json!({"type":"toolCall","id":"call","name":"read","arguments":{},"index":0.0});
    let mut call = call;
    call["index"] = JsValue::Number(f64::NEG_INFINITY);
    call["arguments"]["number"] = JsValue::Number(f64::NAN);
    call["arguments"]["zero"] = JsValue::Number(-0.0);
    value["content"] = JsValue::Array(vec![call.clone()]);
    let message: Message = from_value(&value).unwrap();
    let Message::Assistant(message) = message else {
        panic!("assistant role")
    };
    assert_eq!(message.timestamp, f64::INFINITY);
    assert!(message.usage.input.is_nan());
    for content in [
        from_value::<AssistantContent>(&call).unwrap(),
        message.content[0].clone(),
    ] {
        let AssistantContent::ToolCall(call) = content else {
            panic!("toolCall type")
        };
        assert_eq!(call.index, Some(f64::NEG_INFINITY));
        assert!(call.arguments[&JsString::from("number")].as_f64().unwrap().is_nan());
        assert!(
            call.arguments[&JsString::from("zero")]
                .as_f64()
                .unwrap()
                .is_sign_negative()
        );
    }
}

#[test]
fn native_user_and_text_content_unions_keep_unpaired_units() {
    let text = JsString::from_utf16(vec![0xd800, 0x61, 0xdc00]);
    let content: UserContent = from_value(&JsValue::String(text.clone())).unwrap();
    assert!(matches!(content, UserContent::Text(value) if value == text));
    let block = json!({"type":"text","text":text});
    assert!(
        matches!(from_value::<TextOrImageContent>(&block).unwrap(), TextOrImageContent::Text(value) if value.text == text)
    );
    let content: UserContent = from_value(&JsValue::Array(vec![block])).unwrap();
    assert!(
        matches!(content,UserContent::Blocks(blocks) if matches!(&blocks[0], TextOrImageContent::Text(value) if value.text == text))
    );
}
