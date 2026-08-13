use pi_coding_agent::cli::args::is_valid_thinking_level;

#[test]
fn accepts_exact_thinking_level_literals_in_source_order() {
    let valid_levels = ["off", "minimal", "low", "medium", "high", "xhigh", "max", "ultra"];

    for level in valid_levels {
        assert!(is_valid_thinking_level(level), "expected {level:?} to be valid");
    }
}

#[test]
fn rejects_non_exact_thinking_level_literals() {
    let invalid_levels = [
        "",
        "OFF",
        "uLtRa",
        " off",
        "off ",
        "x-high",
        "unsupported",
        "extreme",
        "é",
        "off\0",
        "off\n",
    ];

    for level in invalid_levels {
        assert!(!is_valid_thinking_level(level), "expected {level:?} to be invalid");
    }
}
