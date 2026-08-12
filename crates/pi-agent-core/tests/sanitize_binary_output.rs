use pi_agent_core::harness::utils::sanitize_binary_output;

#[test]
fn empty_and_ordinary_unicode_are_unchanged() {
    assert_eq!(sanitize_binary_output(""), "");
    assert_eq!(sanitize_binary_output("Hello, 世界! 👋"), "Hello, 世界! 👋");
}

#[test]
fn removes_c0_controls_except_tab_line_feed_and_carriage_return() {
    let input: String = (0..=0x1f).map(|codepoint| char::from(codepoint as u8)).collect();

    assert_eq!(sanitize_binary_output(&input), "\t\n\r");
}

#[test]
fn handles_control_and_printable_boundaries() {
    let input = "\u{001f}\u{0020}\u{007f}\u{0080}";

    assert_eq!(sanitize_binary_output(input), " \u{007f}\u{0080}");
}

#[test]
fn removes_interlinear_annotation_controls_but_preserves_neighbors() {
    let input = "\u{fff8}\u{fff9}\u{fffa}\u{fffb}\u{fffc}";

    assert_eq!(sanitize_binary_output(input), "\u{fff8}\u{fffc}");
}

#[test]
fn preserves_mixture_order_duplicates_and_allowed_whitespace_controls() {
    let input = "a\u{0001}b\t\u{fff9}c\nc\u{000b}\rd\u{fff9}e";

    assert_eq!(sanitize_binary_output(input), "ab\tc\nc\rde");
}

#[test]
fn preserves_c1_controls_and_format_characters() {
    let input = "\u{0080}\u{009f}\u{200b}\u{200d}\u{feff}\u{202e}\u{2060}\u{2066}";

    assert_eq!(sanitize_binary_output(input), input);
}

#[test]
fn preserves_private_use_unassigned_noncharacters_combining_and_variation_scalars() {
    let input = concat!(
        "\u{e000}\u{f8ff}",
        "\u{f0000}\u{10fffd}",
        "\u{0378}\u{2fef}\u{2fa1e}",
        "\u{fdd0}\u{fdef}\u{ffff}\u{1fffe}\u{10ffff}",
        "\u{0301}\u{20d0}\u{fe0f}\u{1f642}",
    );

    assert_eq!(sanitize_binary_output(input), input);
}

#[test]
fn control_only_input_can_become_empty() {
    let input = "\u{0000}\u{0001}\u{000b}\u{000c}\u{001f}\u{fff9}\u{fffa}\u{fffb}";

    assert_eq!(sanitize_binary_output(input), "");
}
