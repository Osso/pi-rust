use pi_protocol::{ImageContent, ImageContentError, TextContent, ThinkingContent};

#[test]
fn text_content_preserves_all_string_values() {
    let cases = ["ordinary text", "", "é", "e\u{301}", "text\0with nul"];

    for value in cases {
        let content = TextContent::new(value.to_owned());
        assert_eq!(content.text(), value);
    }
}

#[test]
fn thinking_content_preserves_text_and_optional_redacted_state() {
    let cases = [
        ("thinking", None),
        ("", Some(false)),
        ("é", Some(true)),
        ("e\u{301}\0", None),
    ];

    for (thinking, redacted) in cases {
        let content = ThinkingContent::new(thinking.to_owned(), redacted);
        assert_eq!(content.thinking(), thinking);
        assert_eq!(content.redacted(), redacted);
    }
}

#[test]
fn image_content_preserves_data_and_mime_without_normalization() {
    let cases = [
        ("base64-data", "image/png"),
        ("", "image/jpeg"),
        ("é\0data", "image/svg+xml"),
        ("raw data", " \t"),
    ];

    for (data, mime_type) in cases {
        let content =
            ImageContent::new(data.to_owned(), mime_type.to_owned()).expect("non-empty MIME types must construct");
        assert_eq!(content.data(), data);
        assert_eq!(content.mime_type(), mime_type);
    }
}

#[test]
fn image_content_rejects_empty_mime_type_with_exact_error() {
    let error = ImageContent::new("data".to_owned(), String::new()).expect_err("empty MIME types must be rejected");

    assert_eq!(error, ImageContentError::EmptyMimeType);
    assert_eq!(error.to_string(), "Image content MIME type must not be empty");
}
