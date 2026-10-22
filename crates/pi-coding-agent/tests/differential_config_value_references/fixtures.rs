use pi_coding_agent::core::{get_config_value_env_var_name, get_config_value_env_var_names, is_command_config_value};
use pi_js_compat::js_json::JsString;
use serde_json::{Value, json};

pub(crate) struct Fixture {
    pub(crate) name: &'static str,
    pub(crate) request: Value,
    pub(crate) expected: Value,
}

struct Expected<'a> {
    env_var_name: Option<&'a str>,
    env_var_names: &'a [&'a str],
    is_command: bool,
}

const INVALID_BRACED_THEN_LATER: &str = "${1TOKEN}$C";
const UNTERMINATED_THEN_LATER: &str = "x${A$B";
const FIRST_CLOSE_NESTED_INVALID: &str = "${A${B}";
const LONGEST_PREFIX: &str = "$A-B$ABC";

pub(crate) fn all() -> Vec<Fixture> {
    [
        exact_and_multiple_references(),
        escapes_and_name_grammar(),
        malformed_and_prefix_references(),
        commands_and_literal_boundaries(),
    ]
    .into_iter()
    .flatten()
    .collect()
}

fn exact_and_multiple_references() -> Vec<Fixture> {
    vec![
        fixture(12050, "exact bare", "$TOKEN", expected(Some("TOKEN"), &["TOKEN"])),
        fixture(12051, "exact braced", "${TOKEN}", expected(Some("TOKEN"), &["TOKEN"])),
        fixture(12052, "literal", "literal", expected(None, &[])),
        fixture(12053, "multiple", "$A-$B-${C}", expected(None, &["A", "B", "C"])),
        fixture(12054, "duplicate order", "$B-$A-$B-${A}", expected(None, &["B", "A"])),
    ]
}

fn escapes_and_name_grammar() -> Vec<Fixture> {
    vec![
        fixture(12055, "combined escapes", "$$-$A-$!-${B}", expected(None, &["A", "B"])),
        fixture(12056, "repeated dollars", "$$$A", expected(None, &["A"])),
        fixture(12057, "invalid bare start", "$1TOKEN", expected(None, &[])),
        fixture(12058, "underscore", "$_TOKEN", expected(Some("_TOKEN"), &["_TOKEN"])),
        fixture(12059, "digit continuation", "$A1", expected(Some("A1"), &["A1"])),
    ]
}

fn malformed_and_prefix_references() -> Vec<Fixture> {
    vec![
        fixture(
            12060,
            "invalid braced then later reference",
            INVALID_BRACED_THEN_LATER,
            expected(None, &["C"]),
        ),
        fixture(
            12061,
            "unterminated then later reference",
            UNTERMINATED_THEN_LATER,
            expected(None, &["B"]),
        ),
        fixture(
            12062,
            "first-close nested invalid",
            FIRST_CLOSE_NESTED_INVALID,
            expected(None, &[]),
        ),
        fixture(12063, "longest prefix", LONGEST_PREFIX, expected(None, &["A", "ABC"])),
    ]
}

fn commands_and_literal_boundaries() -> Vec<Fixture> {
    vec![
        fixture(12064, "command boundary", "!", command_expected()),
        fixture(12065, "command precedence", "!echo $TOKEN", command_expected()),
        fixture(
            12066,
            "Unicode supplementary NUL",
            "pré😀\0$A${B}尾",
            expected(None, &["A", "B"]),
        ),
        fixture(12067, "empty", "", expected(None, &[])),
        fixture(12068, "lone dollar", "$", expected(None, &[])),
    ]
}

const fn expected<'a>(env_var_name: Option<&'a str>, env_var_names: &'a [&'a str]) -> Expected<'a> {
    Expected {
        env_var_name,
        env_var_names,
        is_command: false,
    }
}

const fn command_expected<'a>() -> Expected<'a> {
    Expected {
        env_var_name: None,
        env_var_names: &[],
        is_command: true,
    }
}

fn fixture(id: u64, name: &'static str, config: &'static str, expected: Expected<'_>) -> Fixture {
    let env_var_name = get_config_value_env_var_name(&JsString::from(config));
    let env_var_names = get_config_value_env_var_names(&JsString::from(config));
    let is_command = is_command_config_value(&JsString::from(config));
    assert_fixture_expectation(name, env_var_name, &env_var_names, is_command, &expected);

    Fixture {
        name,
        request: json!({"id": id, "config": config}),
        expected: json!({
            "id": id,
            "ok": true,
            "output": {
                "envVarName": env_var_name,
                "envVarNames": env_var_names,
                "isCommand": is_command,
            },
        }),
    }
}

fn assert_fixture_expectation(
    name: &str,
    env_var_name: Option<&str>,
    env_var_names: &[String],
    is_command: bool,
    expected: &Expected<'_>,
) {
    let expected_names = expected
        .env_var_names
        .iter()
        .map(|name| (*name).to_owned())
        .collect::<Vec<_>>();

    assert_eq!(env_var_name, expected.env_var_name, "fixture expectation drift: {name}");
    assert_eq!(env_var_names, expected_names, "fixture expectation drift: {name}");
    assert_eq!(is_command, expected.is_command, "fixture expectation drift: {name}");
}
