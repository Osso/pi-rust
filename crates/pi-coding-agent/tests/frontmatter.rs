use pi_coding_agent::utils::frontmatter::{parse_frontmatter, strip_frontmatter};
use serde_json::json;

#[test]
fn parses_keys_strips_quotes_and_returns_body() {
    let input = "---\nname: \"skill-name\"\ndescription: 'A desc'\nfoo-bar: value\n---\n\nBody text";
    let parsed = parse_frontmatter(input).expect("valid frontmatter");
    assert_eq!(
        parsed.frontmatter,
        json!({"name": "skill-name", "description": "A desc", "foo-bar": "value"}).into()
    );
    assert_eq!(parsed.body, "Body text");
}

#[test]
fn normalizes_newlines_and_handles_crlf() {
    let input = "---\r\nname: test\r\n---\r\nLine one\r\nLine two";
    let parsed = parse_frontmatter(input).expect("valid frontmatter");
    assert_eq!(parsed.frontmatter, json!({"name": "test"}).into());
    assert_eq!(parsed.body, "Line one\nLine two");
}

#[test]
fn rejects_invalid_yaml() {
    let error = parse_frontmatter("---\nfoo: [bar\n---\nBody").expect_err("invalid YAML");
    assert!(error.to_string().contains("line"), "{error:?}");
}

#[test]
fn parses_block_scalars() {
    let input = "---\ndescription: |\n  Line one\n  Line two\n---\n\nBody";
    let parsed = parse_frontmatter(input).expect("valid frontmatter");
    assert_eq!(
        parsed.frontmatter,
        json!({"description": "Line one\nLine two\n"}).into()
    );
    assert_eq!(parsed.body, "Body");
}

#[test]
fn returns_original_content_when_frontmatter_is_missing_or_unterminated() {
    let no_frontmatter = "Just text\nsecond line";
    let missing_end = "---\nname: test\nBody without terminator";

    let parsed = parse_frontmatter(no_frontmatter).expect("no frontmatter");
    assert_eq!(parsed.frontmatter, json!({}).into());
    assert_eq!(parsed.body, "Just text\nsecond line");

    let parsed = parse_frontmatter(missing_end).expect("unterminated frontmatter");
    assert_eq!(parsed.frontmatter, json!({}).into());
    assert_eq!(parsed.body, "---\nname: test\nBody without terminator");
}

#[test]
fn returns_empty_object_for_empty_or_comment_only_frontmatter() {
    let parsed = parse_frontmatter("---\n# just a comment\n---\nBody").expect("comment-only");
    assert_eq!(parsed.frontmatter, json!({}).into());
    assert_eq!(parsed.body, "Body");

    let parsed = parse_frontmatter("---\n---\nBody").expect("empty frontmatter");
    assert_eq!(parsed.frontmatter, json!({}).into());
    assert_eq!(parsed.body, "Body");
}

#[test]
fn parses_nested_structures_and_scalars() {
    let input = "---\nlist:\n  - one\n  - two\nnested:\n  enabled: true\n  count: 3\n  empty: null\n---\nBody";
    let parsed = parse_frontmatter(input).expect("valid frontmatter");
    assert_eq!(
        parsed.frontmatter,
        json!({
            "list": ["one", "two"],
            "nested": {"enabled": true, "count": 3, "empty": null}
        })
        .into()
    );
}

#[test]
fn strip_removes_frontmatter_and_trims_body() {
    assert_eq!(
        strip_frontmatter("---\nkey: value\n---\n\nBody\n").expect("valid"),
        "Body"
    );
}

#[test]
fn strip_returns_untrimmed_body_when_no_frontmatter_is_present() {
    assert_eq!(
        strip_frontmatter("\n  No frontmatter body  \n").expect("no frontmatter"),
        "\n  No frontmatter body  \n"
    );
}
