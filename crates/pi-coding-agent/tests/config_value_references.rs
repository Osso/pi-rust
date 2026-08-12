use pi_coding_agent::core::{get_config_value_env_var_name, get_config_value_env_var_names, is_command_config_value};

fn assert_names(value: &str, expected: &[&str]) {
    let actual = get_config_value_env_var_names(value);

    assert_eq!(actual.len(), expected.len(), "unexpected names for {value:?}");
    for (actual_name, expected_name) in actual.iter().zip(expected) {
        assert_eq!(
            actual_name.to_string(),
            *expected_name,
            "unexpected names for {value:?}"
        );
    }
}

fn assert_no_single_name(value: &str) {
    assert_eq!(
        get_config_value_env_var_name(value),
        None,
        "unexpected single reference for {value:?}"
    );
}

#[test]
fn exact_single_references_return_borrowed_name_and_one_name_list() {
    let cases = [
        ("$TOKEN", "TOKEN"),
        ("${TOKEN}", "TOKEN"),
        ("$_TOKEN", "_TOKEN"),
        ("${_A1}", "_A1"),
        ("$A1", "A1"),
    ];

    for (value, expected_name) in cases {
        assert_eq!(get_config_value_env_var_name(value), Some(expected_name));
        assert_names(value, &[expected_name]);
        assert!(!is_command_config_value(value));
    }
}

#[test]
fn single_name_helper_requires_the_entire_value_to_be_one_reference() {
    let cases = [
        "literal",
        "prefix $TOKEN",
        "$TOKEN suffix",
        "$A$B",
        "$A$A",
        "$$TOKEN",
        "$!TOKEN",
        "${1A}",
        "!echo $TOKEN",
    ];

    for value in cases {
        assert_no_single_name(value);
    }
}

#[test]
fn unbraced_names_use_ascii_grammar_longest_prefix_and_case_sensitivity() {
    let cases = [
        ("$A-B$ABC", &["A", "ABC"][..]),
        ("$A.B", &["A"][..]),
        ("$A1x", &["A1x"][..]),
        ("$_X", &["_X"][..]),
        ("$a$A", &["a", "A"][..]),
        ("$1A", &[][..]),
        ("$Ä", &[][..]),
        ("$😀", &[][..]),
        ("${1A}", &[][..]),
        ("${Ä}", &[][..]),
    ];

    for (value, expected_names) in cases {
        assert_names(value, expected_names);
    }
}

#[test]
fn duplicate_names_are_removed_without_changing_first_seen_order() {
    assert_names("$B-$A-$B-${A}-$C-$B", &["B", "A", "C"]);
}

#[test]
fn escaped_and_repeated_dollars_only_expose_unescaped_references() {
    let cases = [
        ("$$TOKEN", &[][..]),
        ("$!TOKEN", &[][..]),
        ("$$$A", &["A"][..]),
        ("$$$$A", &[][..]),
        ("$$-$A-$!-${B}", &["A", "B"][..]),
    ];

    for (value, expected_names) in cases {
        assert_names(value, expected_names);
    }
}

#[test]
fn braced_references_validate_names_and_stop_at_the_first_close() {
    let cases = [
        ("${A}", &["A"][..]),
        ("${}", &[][..]),
        ("${A-B}", &[][..]),
        ("${A-B}$C", &["C"][..]),
        ("${A B}", &[][..]),
        ("${A}x}", &["A"][..]),
        ("${A${B}", &[][..]),
        ("$A}", &["A"][..]),
    ];

    for (value, expected_names) in cases {
        assert_names(value, expected_names);
    }
}

#[test]
fn unterminated_braces_are_rescanned_after_the_opening_dollar() {
    let cases = [("${A", &[][..]), ("x${A$B", &["B"][..]), ("${A$B$C", &["B", "C"][..])];

    for (value, expected_names) in cases {
        assert_names(value, expected_names);
    }
}

#[test]
fn leading_exclamation_takes_command_precedence_over_templates() {
    let commands = ["!", "!echo $A", "!!echo", "!$A"];

    for value in commands {
        assert!(is_command_config_value(value), "expected command: {value:?}");
        assert_names(value, &[]);
        assert_no_single_name(value);
    }

    for value in ["$!echo", "x!", ""] {
        assert!(!is_command_config_value(value), "unexpected command: {value:?}");
    }
}

#[test]
fn empty_lone_and_unrecognized_dollars_have_no_references() {
    for value in ["", "$", "$-", "$9"] {
        assert_names(value, &[]);
        assert_no_single_name(value);
        assert!(!is_command_config_value(value));
    }
}

#[test]
fn unicode_literals_nul_and_case_distinct_names_are_preserved() {
    assert_names("pré😀\0$A${B}尾", &["A", "B"]);
    assert_names("$É", &[]);
    assert_names("$a$A", &["a", "A"]);
}
