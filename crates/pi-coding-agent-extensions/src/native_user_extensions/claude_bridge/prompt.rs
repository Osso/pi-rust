//! Source index.ts trailing-user-turn and image-preserving prompt extraction.
use super::stream::string;
use pi_ai::types::{Context, Message};
use pi_js_compat::{
    callback_error::CallbackError,
    js_json::{self, JsString, JsValue},
};

pub(super) struct Prompt {
    pub text: JsString,
    pub blocks: Vec<JsValue>,
}
impl Prompt {
    pub fn has_images(&self) -> bool {
        self.blocks.iter().any(|b| string(b, "type") == "image")
    }
    pub fn into_content(self) -> JsValue {
        if self.has_images() {
            return JsValue::Array(self.blocks);
        }
        let text = if self.text.is_empty() {
            "[continue]".into()
        } else {
            self.text
        };
        JsValue::Array(vec![JsValue::object([
            ("type", JsValue::String("text".into())),
            ("text", JsValue::String(text)),
        ])])
    }
}
pub(super) fn turn_contents(context: &Context) -> Result<Vec<JsValue>, CallbackError> {
    let mut start = context.messages.len();
    while start > 0 && matches!(context.messages[start - 1], Message::User(_)) {
        start -= 1;
    }
    context.messages[start..]
        .iter()
        .map(|message| match message {
            Message::User(user) => js_json::to_value(&user.content).map_err(|e| CallbackError::new(e.to_string())),
            _ => Err(CallbackError::new("Expected a user message in the current prompt turn")),
        })
        .collect()
}
pub(super) fn valid_image(block: &JsValue) -> bool {
    !string(block, "data").is_empty() && !string(block, "mimeType").is_empty()
}
pub(super) fn extract(context: &Context) -> Result<Prompt, CallbackError> {
    let contents = turn_contents(context)?;
    let text = JsString::join(
        contents
            .iter()
            .map(super::convert::content_text)
            .filter(|s| !s.is_empty()),
        "\n",
    );
    let mut blocks = Vec::new();
    for content in contents {
        let input = match content {
            JsValue::String(text) => vec![JsValue::object([
                ("type", JsValue::String("text".into())),
                ("text", JsValue::String(text)),
            ])],
            JsValue::Array(blocks) => blocks,
            _ => {
                return Err(CallbackError::new(
                    "extractUserPromptBlocks: user message content must be a string or block array",
                ));
            }
        };
        for block in input {
            let kind = string(&block, "type");
            if kind == "text" && !string(&block, "text").is_empty() {
                blocks.push(JsValue::object([
                    ("type", JsValue::String("text".into())),
                    ("text", JsValue::String(string(&block, "text"))),
                ]));
            } else if kind == "image" && valid_image(&block) {
                blocks.push(JsValue::object([
                    ("type", JsValue::String("image".into())),
                    (
                        "source",
                        JsValue::object([
                            ("type", JsValue::String("base64".into())),
                            ("media_type", JsValue::String(string(&block, "mimeType"))),
                            ("data", JsValue::String(string(&block, "data"))),
                        ]),
                    ),
                ]));
            }
        }
    }
    Ok(Prompt { text, blocks })
}
