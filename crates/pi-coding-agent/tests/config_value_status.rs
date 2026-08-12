use std::collections::HashMap;
use std::ffi::OsString;

use serde_json::json;

#[path = "support/config_value_status_probe.rs"]
mod config_value_status_probe;

use config_value_status_probe::{
    ENV_A, ENV_B, ENV_C, ENV_LOWERCASE_A, normalized_status, run_child_probe, spawn_child_probe,
};

#[test]
fn literals_and_commands_are_configured_without_execution() {
    for value in ["", "literal", "$$A", "$!A", "!", "!echo $A"] {
        assert_eq!(
            normalized_status(value, None),
            json!({"missing": [], "configured": true})
        );
    }
}

#[test]
fn explicit_values_use_nonempty_truthiness_and_preserve_bytes() {
    let mut values = HashMap::new();
    values.insert(ENV_A.to_owned(), "0".to_owned());
    values.insert(ENV_B.to_owned(), " \t".to_owned());
    assert_eq!(
        normalized_status(
            "$PI_RUST_CONFIG_STATUS_A_7D9F-$PI_RUST_CONFIG_STATUS_B_7D9F-$PI_RUST_CONFIG_STATUS_A_7D9F",
            Some(&values)
        ),
        json!({"missing": [], "configured": true})
    );

    values.insert(ENV_A.to_owned(), "é世界🙂\0".to_owned());
    values.insert(ENV_B.to_owned(), "尾\0".to_owned());
    assert_eq!(
        normalized_status(
            "$PI_RUST_CONFIG_STATUS_A_7D9F${PI_RUST_CONFIG_STATUS_B_7D9F}",
            Some(&values)
        ),
        json!({"missing": [], "configured": true})
    );
}

#[test]
fn child_process_scenarios_match_process_environment_status() {
    let scenarios = [
        ("missing", vec![], json!({"missing": [ENV_A], "configured": false})),
        (
            "process_empty",
            vec![(ENV_A, OsString::from(""))],
            json!({"missing": [ENV_A], "configured": false}),
        ),
        (
            "process_ascii",
            vec![(ENV_A, OsString::from("value"))],
            json!({"missing": [], "configured": true}),
        ),
        (
            "process_unicode",
            vec![(ENV_A, OsString::from("é世界🙂"))],
            json!({"missing": [], "configured": true}),
        ),
        (
            "explicit_empty_fallback",
            vec![(ENV_A, OsString::from("process-value"))],
            json!({"missing": [], "configured": true}),
        ),
        (
            "mixed",
            vec![(ENV_A, OsString::from("process-a"))],
            json!({"missing": [ENV_C], "configured": false}),
        ),
        (
            "explicit_case",
            vec![],
            json!({"missing": [ENV_LOWERCASE_A], "configured": false}),
        ),
    ];

    for (mode, env_pairs, expected) in scenarios {
        assert_eq!(spawn_child_probe(mode, &env_pairs), expected, "scenario: {mode}");
    }

    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStringExt;

        let expected = json!({"missing": [], "configured": true});
        let env_pairs = vec![(ENV_A, OsString::from_vec(vec![0xff]))];
        assert_eq!(
            spawn_child_probe("process_nonunicode", &env_pairs),
            expected,
            "scenario: process_nonunicode"
        );
    }
}

#[test]
#[ignore]
fn config_status_process_probe_child() {
    run_child_probe();
}
