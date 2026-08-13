use pi_coding_agent::{ParsedSkillBlock, parse_skill_block};

fn assert_block(block: ParsedSkillBlock, name: &str, location: &str, content: &str, user_message: Option<&str>) {
    assert_eq!(block.name, name);
    assert_eq!(block.location, location);
    assert_eq!(block.content, content);
    assert_eq!(block.user_message.as_deref(), user_message);
}

#[test]
fn parses_minimal_and_multiline_skill_blocks_without_messages() {
    let minimal =
        parse_skill_block("<skill name=\"n\" location=\"l\">\n\n</skill>").expect("minimal skill block should parse");
    assert_block(minimal, "n", "l", "", None);

    let multiline = parse_skill_block("<skill name=\"demo\" location=\"/tmp/SKILL.md\">\nfirst\nsecond\n</skill>")
        .expect("multiline skill block should parse");
    assert_block(multiline, "demo", "/tmp/SKILL.md", "first\nsecond", None);
}

#[test]
fn parses_message_and_applies_javascript_trim_semantics() {
    let input = "<skill name=\"demo\" location=\"loc\">\ncontent\n</skill>\n\n\u{feff}\t user message \u{00a0}\n";
    let block = parse_skill_block(input).expect("skill block with a message should parse");
    assert_block(block, "demo", "loc", "content", Some("user message"));
}

#[test]
fn whitespace_only_message_uses_javascript_trim_set() {
    for whitespace in [" ", "\t\n\r", "\u{feff}"] {
        let input = format!("<skill name=\"n\" location=\"l\">\nx\n</skill>\n\n{whitespace}");
        let block = parse_skill_block(&input).expect("whitespace suffix should parse");
        assert_block(block, "n", "l", "x", None);
    }

    let input = "<skill name=\"n\" location=\"l\">\nx\n</skill>\n\n\u{001c}";
    let block = parse_skill_block(input).expect("non-ECMAScript whitespace should parse");
    assert_block(block, "n", "l", "x", Some("\u{001c}"));
}

#[test]
fn preserves_unicode_nul_and_newlines_where_the_source_pattern_permits_them() {
    let input = "<skill name=\"na\nme\0😀\" location=\"lo\ncation\0é\">\nα\0\n😀\n</skill>";
    let block = parse_skill_block(input).expect("representable source text should parse");
    assert_block(block, "na\nme\0😀", "lo\ncation\0é", "α\0\n😀", None);
}

#[test]
fn requires_exact_delimiters_and_full_input_anchoring() {
    let valid = "<skill name=\"n\" location=\"l\">\nx\n</skill>";
    for malformed in [
        "<skill name=\"n\" location=\"l\">\r\nx\n</skill>",
        "<skill name=\"n\" location=\"l\">\nx\n</skill>\nmessage",
        "prefix<skill name=\"n\" location=\"l\">\nx\n</skill>",
        "<skill name=\"n\" location=\"l\">\nx\n</skill>suffix",
        "<skill name=\"n\" location=\"l\">\nx\n</skill>\n\n",
        "<skill name=\"n\" location=\"l\">x</skill>",
    ] {
        assert!(
            parse_skill_block(malformed).is_none(),
            "unexpected parse: {malformed:?}"
        );
    }
    assert!(parse_skill_block(valid).is_some());
    let content_cr = "<skill name=\"n\" location=\"l\">\nx\r\n</skill>";
    assert_eq!(parse_skill_block(content_cr).expect("CR may be content").content, "x\r");
}

#[test]
fn rejects_empty_or_quoted_attributes_and_missing_close_delimiter() {
    for malformed in [
        "<skill name=\"\" location=\"l\">\nx\n</skill>",
        "<skill name=\"n\" location=\"\">\nx\n</skill>",
        "<skill name=\"n\"b\" location=\"l\">\nx\n</skill>",
        "<skill name=\"n\" location=\"l\" extra=\"x\">\nx\n</skill>",
        "<skill name=\"n\" location=\"l\">\nx</skill>",
    ] {
        assert!(
            parse_skill_block(malformed).is_none(),
            "unexpected parse: {malformed:?}"
        );
    }
}

#[test]
fn backtracks_to_a_later_closing_delimiter_when_the_first_remainder_is_invalid() {
    let input = "<skill name=\"n\" location=\"l\">\nfirst\n</skill>\nsecond\n</skill>";
    let block = parse_skill_block(input).expect("later closing delimiter should complete the match");
    assert_block(block, "n", "l", "first\n</skill>\nsecond", None);
}

#[test]
fn accepts_only_a_nonempty_double_lf_message_suffix() {
    let message = parse_skill_block("<skill name=\"n\" location=\"l\">\nx\n</skill>\n\ny")
        .expect("double-LF message suffix should parse");
    assert_eq!(message.user_message.as_deref(), Some("y"));

    for malformed in [
        "<skill name=\"n\" location=\"l\">\nx\n</skill>\ny",
        "<skill name=\"n\" location=\"l\">\nx\n</skill>\n\n",
    ] {
        assert!(
            parse_skill_block(malformed).is_none(),
            "unexpected parse: {malformed:?}"
        );
    }
}
