use pi_coding_agent::utils::deprecation::{clear_deprecation_warnings_for_tests, emit_deprecation};

#[test]
fn emits_each_message_once_until_cleared() {
    clear_deprecation_warnings_for_tests();
    let mut output = Vec::new();

    assert!(emit_deprecation(&mut output, "first warning"));
    assert!(!emit_deprecation(&mut output, "first warning"));
    assert!(emit_deprecation(&mut output, "second warning"));

    assert_eq!(
        String::from_utf8(output).expect("UTF-8 warning output"),
        "Deprecation warning: first warning\nDeprecation warning: second warning\n"
    );

    clear_deprecation_warnings_for_tests();
    let mut output = Vec::new();
    assert!(emit_deprecation(&mut output, "first warning"));
}
