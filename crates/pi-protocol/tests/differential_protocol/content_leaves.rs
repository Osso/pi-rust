use pi_protocol::{ImageContent, TextContent, ThinkingContent};
use serde_json::{Value, json};

use super::oracle::TsOracle;

const FIRST_REQUEST_ID: u64 = 5000;
const FIXTURE_COUNT: u64 = 17;

struct TextFixture {
    name: &'static str,
    tag: &'static str,
    text: &'static str,
}

impl TextFixture {
    const fn new(name: &'static str, tag: &'static str, text: &'static str) -> Self {
        Self { name, tag, text }
    }

    fn request(&self, id: u64) -> Value {
        json!({
            "id": id,
            "operation": "validate_text_content",
            "value": {
                "type": self.tag,
                "text": self.text,
            },
        })
    }

    fn rust_response(&self, id: u64) -> Value {
        if self.tag != "text" {
            return invalid_response(id);
        }
        let content = TextContent::new(self.text.to_owned());
        valid_response(
            id,
            json!({
                "type": "text",
                "text": content.text(),
            }),
        )
    }
}

struct ThinkingFixture {
    name: &'static str,
    tag: &'static str,
    thinking: &'static str,
    redacted: Option<bool>,
}

impl ThinkingFixture {
    const fn new(name: &'static str, tag: &'static str, thinking: &'static str, redacted: Option<bool>) -> Self {
        Self {
            name,
            tag,
            thinking,
            redacted,
        }
    }

    fn request(&self, id: u64) -> Value {
        let mut value = json!({
            "type": self.tag,
            "thinking": self.thinking,
        });
        if let Some(redacted) = self.redacted {
            value["redacted"] = json!(redacted);
        }
        json!({
            "id": id,
            "operation": "validate_thinking_content",
            "value": value,
        })
    }

    fn rust_response(&self, id: u64) -> Value {
        if self.tag != "thinking" {
            return invalid_response(id);
        }
        let content = ThinkingContent::new(self.thinking.to_owned(), self.redacted);
        let mut value = json!({
            "type": "thinking",
            "thinking": content.thinking(),
        });
        if let Some(redacted) = content.redacted() {
            value["redacted"] = json!(redacted);
        }
        valid_response(id, value)
    }
}

struct ImageFixture {
    name: &'static str,
    tag: &'static str,
    data: &'static str,
    mime_type: &'static str,
}

impl ImageFixture {
    const fn new(name: &'static str, tag: &'static str, data: &'static str, mime_type: &'static str) -> Self {
        Self {
            name,
            tag,
            data,
            mime_type,
        }
    }

    fn request(&self, id: u64) -> Value {
        json!({
            "id": id,
            "operation": "validate_image_content",
            "value": {
                "type": self.tag,
                "data": self.data,
                "mimeType": self.mime_type,
            },
        })
    }

    fn rust_response(&self, id: u64) -> Value {
        if self.tag != "image" {
            return invalid_response(id);
        }
        let Some(content) = ImageContent::new(self.data.to_owned(), self.mime_type.to_owned()).ok() else {
            return invalid_response(id);
        };
        valid_response(
            id,
            json!({
                "type": "image",
                "data": content.data(),
                "mimeType": content.mime_type(),
            }),
        )
    }
}

const TEXT_FIXTURES: &[TextFixture] = &[
    TextFixture::new("ordinary", "text", "hello"),
    TextFixture::new("empty", "text", ""),
    TextFixture::new("Unicode, combining, supplementary", "text", "é e\u{301} 𐅑"),
    TextFixture::new("embedded NUL", "text", "text\0with nul"),
    TextFixture::new("wrong tag", "thinking", "hello"),
];

const THINKING_FIXTURES: &[ThinkingFixture] = &[
    ThinkingFixture::new("ordinary omitted redacted", "thinking", "reasoning", None),
    ThinkingFixture::new("empty omitted redacted", "thinking", "", None),
    ThinkingFixture::new("Unicode and NUL omitted redacted", "thinking", "é\0𐅑", None),
    ThinkingFixture::new("explicit false", "thinking", "reasoning", Some(false)),
    ThinkingFixture::new("explicit true", "thinking", "reasoning", Some(true)),
    ThinkingFixture::new("wrong tag", "text", "reasoning", None),
];

const IMAGE_FIXTURES: &[ImageFixture] = &[
    ImageFixture::new("ordinary", "image", "base64-data", "image/png"),
    ImageFixture::new("empty data", "image", "", "image/jpeg"),
    ImageFixture::new("Unicode and NUL data", "image", "é\0𐅑", "image/svg+xml"),
    ImageFixture::new("whitespace-only MIME", "image", "data", " \t"),
    ImageFixture::new("wrong tag", "text", "data", "image/png"),
    ImageFixture::new("empty MIME", "image", "data", ""),
];

fn valid_response(id: u64, value: Value) -> Value {
    json!({
        "id": id,
        "ok": true,
        "output": {
            "valid": true,
            "value": value,
        },
    })
}

fn invalid_response(id: u64) -> Value {
    json!({
        "id": id,
        "ok": true,
        "output": {"valid": false},
    })
}

pub(crate) fn compare_all_fixtures(oracle: &mut TsOracle) {
    let mut id = FIRST_REQUEST_ID;
    for fixture in TEXT_FIXTURES {
        let expected = oracle.request(&fixture.request(id));
        let actual = fixture.rust_response(id);
        assert_eq!(actual, expected, "text-content mismatch: {}", fixture.name);
        id += 1;
    }
    for fixture in THINKING_FIXTURES {
        let expected = oracle.request(&fixture.request(id));
        let actual = fixture.rust_response(id);
        assert_eq!(actual, expected, "thinking-content mismatch: {}", fixture.name);
        id += 1;
    }
    for fixture in IMAGE_FIXTURES {
        let expected = oracle.request(&fixture.request(id));
        let actual = fixture.rust_response(id);
        assert_eq!(actual, expected, "image-content mismatch: {}", fixture.name);
        id += 1;
    }
    assert_eq!(id, FIRST_REQUEST_ID + FIXTURE_COUNT);
}
