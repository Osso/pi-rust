use pi_coding_agent::utils::chalk::{Chalk, Level};
use pi_coding_agent::utils::deprecation::{clear_deprecation_warnings_for_tests, emit_deprecation_with};

// One test: the dedupe set and its clear are process-wide, so parallel tests
// would race on the same messages.
#[test]
fn emits_each_message_once_until_cleared() {
    clear_deprecation_warnings_for_tests();
    let plain = Chalk::new(Level::Plain);
    let mut output = Vec::new();

    assert!(emit_deprecation_with(&plain, &mut output, "first warning"));
    assert!(!emit_deprecation_with(&plain, &mut output, "first warning"));
    assert!(emit_deprecation_with(&plain, &mut output, "second warning"));

    assert_eq!(
        String::from_utf8(output).expect("UTF-8 warning output"),
        "Deprecation warning: first warning\nDeprecation warning: second warning\n"
    );

    clear_deprecation_warnings_for_tests();
    let mut output = Vec::new();
    assert!(emit_deprecation_with(&plain, &mut output, "first warning"));

    clear_deprecation_warnings_for_tests();
    let mut output = Vec::new();
    assert!(emit_deprecation_with(&Chalk::new(Level::Basic), &mut output, "styled"));
    assert_eq!(
        String::from_utf8(output).expect("UTF-8 warning output"),
        "\x1b[33mDeprecation warning: styled\x1b[39m\n"
    );

    clear_deprecation_warnings_for_tests();
}
