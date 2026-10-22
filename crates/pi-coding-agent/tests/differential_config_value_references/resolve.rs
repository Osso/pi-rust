//! Differential proof for `resolveConfigValue`, `resolveConfigValueUncached`,
//! `resolveConfigValueOrThrow`, `resolveHeaders`, and `resolveHeadersOrThrow`.

use pi_js_compat::js_json::JsString;
use std::collections::HashMap;

use pi_coding_agent::core::{
    clear_config_value_cache, resolve_config_value, resolve_config_value_or_throw, resolve_config_value_uncached,
    resolve_headers, resolve_headers_or_throw,
};
use serde_json::{Value, json};

use crate::config_value_reference_oracle::oracle::TsOracle;
use crate::config_value_reference_oracle::resolve_probe;

const PROCESS: &str = "PI_DIFF_RESOLVE_PROCESS";

type EnvPairs<'a> = &'a [(&'a str, &'a str)];

struct ThrowCase<'a> {
    request: Value,
    env: Option<EnvPairs<'a>>,
}

struct HeaderCase<'a> {
    request: Value,
    values: Option<EnvPairs<'a>>,
    env: Option<EnvPairs<'a>>,
    throw_on_error: bool,
}

fn env(pairs: EnvPairs<'_>) -> HashMap<String, JsString> {
    pairs
        .iter()
        .map(|(key, value)| ((*key).to_owned(), JsString::from(*value)))
        .collect()
}

fn optional_env(pairs: Option<EnvPairs<'_>>) -> Option<HashMap<String, JsString>> {
    pairs.map(env)
}

fn json_headers(headers: Option<HashMap<String, JsString>>) -> Value {
    headers.map_or(Value::Null, |headers| json!(headers))
}

fn resolve_request(
    id: usize,
    config: &str,
    explicit: Option<EnvPairs<'_>>,
    process_env: Option<EnvPairs<'_>>,
    uncached: bool,
) -> Value {
    let mut request = json!({
        "id": id,
        "operation": "resolve",
        "config": config,
        "env": explicit.map(|pairs| json!(env(pairs))),
        "uncached": uncached,
    });
    if let Some(pairs) = process_env {
        request["processEnv"] = json!(env(pairs));
    }
    request
}

fn throw_request(id: usize, config: &str, description: &str, explicit: Option<EnvPairs<'_>>) -> Value {
    json!({
        "id": id,
        "operation": "throw",
        "config": config,
        "description": description,
        "env": explicit.map(|pairs| json!(env(pairs))),
    })
}

fn headers_request(
    id: usize,
    values: Option<EnvPairs<'_>>,
    explicit: Option<EnvPairs<'_>>,
    throw_on_error: bool,
) -> Value {
    json!({
        "id": id,
        "operation": "headers",
        "headers": values.map(|pairs| json!(env(pairs))),
        "description": "Provider",
        "env": explicit.map(|pairs| json!(env(pairs))),
        "throwOnError": throw_on_error,
    })
}

pub(crate) fn run(oracle: &mut TsOracle) {
    clear_config_value_cache();
    let explicit = [("PI_DIFF_RESOLVE_A", "left"), ("PI_DIFF_RESOLVE_B", "right")];
    let mut id = 0;
    let mut next = || {
        id += 1;
        id
    };

    let resolve_cases = vec![
        resolve_request(next(), "literal-key", None, None, false),
        resolve_request(next(), "$PI_DIFF_RESOLVE_A", Some(&explicit), None, false),
        resolve_request(
            next(),
            "${PI_DIFF_RESOLVE_A}_$PI_DIFF_RESOLVE_B",
            Some(&explicit),
            None,
            false,
        ),
        resolve_request(next(), "$$PI_DIFF_RESOLVE_A", Some(&explicit), None, false),
        resolve_request(next(), "$!literal-$PI_DIFF_RESOLVE_B", Some(&explicit), None, false),
        resolve_request(next(), "$PI_DIFF_RESOLVE_ABSENT", Some(&explicit), None, false),
        resolve_request(next(), "!printf '  spaced  '", None, None, false),
        resolve_request(next(), "!printf 'line1\\nline2'", None, None, false),
        resolve_request(next(), "!echo 'hello world' | tr ' ' '-'", None, None, false),
        resolve_request(next(), "!exit 3", None, None, false),
        resolve_request(next(), "!printf ''", None, None, false),
        resolve_request(next(), "!printf 'uncached'", None, None, true),
    ];
    for request in &resolve_cases {
        let expected = oracle.request(request);
        assert_eq!(expected["ok"], true, "oracle failed: {expected}");
        let config = request["config"].as_str().unwrap();
        let explicit_env = request["env"].as_object().map(|object| {
            object
                .iter()
                .map(|(key, value)| (key.clone(), JsString::from(value.as_str().unwrap())))
                .collect::<HashMap<_, _>>()
        });
        let uncached = request["uncached"].as_bool().unwrap_or(false);
        let actual = if uncached {
            resolve_config_value_uncached(&JsString::from(config), explicit_env.as_ref())
        } else {
            resolve_config_value(&JsString::from(config), explicit_env.as_ref())
        };
        assert_eq!(
            json!({"value": actual}),
            expected["output"],
            "resolve case {config:?} (explicit env {explicit_env:?})"
        );
    }

    // Cases that read the process environment run in a child process, like the status probes.
    let process_cases = [
        (
            "process_only",
            resolve_request(
                next(),
                "$PI_DIFF_RESOLVE_PROCESS",
                None,
                Some(&[(PROCESS, "process")]),
                false,
            ),
        ),
        (
            "explicit_beats_process",
            resolve_request(
                next(),
                "$PI_DIFF_RESOLVE_PROCESS",
                Some(&[(PROCESS, "explicit")]),
                Some(&[(PROCESS, "process")]),
                false,
            ),
        ),
        (
            "empty_explicit_falls_back",
            resolve_request(
                next(),
                "$PI_DIFF_RESOLVE_PROCESS",
                Some(&[(PROCESS, "")]),
                Some(&[(PROCESS, "fallback")]),
                false,
            ),
        ),
    ];
    for (mode, request) in &process_cases {
        let expected = oracle.request(request);
        assert_eq!(expected["ok"], true, "oracle failed: {expected}");
        let process_value = request["processEnv"][PROCESS].as_str().unwrap();
        let actual = resolve_probe::spawn(mode, &[(PROCESS, process_value.into())]);
        assert_eq!(actual, expected["output"], "process case {mode}");
    }

    let throw_cases = vec![
        ThrowCase {
            request: throw_request(next(), "!exit 1", "API key", None),
            env: None,
        },
        ThrowCase {
            request: throw_request(next(), "$PI_DIFF_RESOLVE_ABSENT", "API key", Some(&explicit)),
            env: Some(&explicit),
        },
        ThrowCase {
            request: throw_request(
                next(),
                "$PI_DIFF_RESOLVE_ABSENT-$PI_DIFF_RESOLVE_MISSING",
                "API key",
                Some(&explicit),
            ),
            env: Some(&explicit),
        },
        ThrowCase {
            request: throw_request(next(), "$PI_DIFF_RESOLVE_A", "API key", Some(&explicit)),
            env: Some(&explicit),
        },
    ];
    for case in &throw_cases {
        let expected = oracle.request(&case.request);
        let config = case.request["config"].as_str().unwrap();
        match resolve_config_value_or_throw(
            &JsString::from(config),
            &JsString::from("API key"),
            optional_env(case.env).as_ref(),
        ) {
            Ok(value) => assert_eq!(json!({"value": value}), expected["output"], "throw case {config:?}"),
            Err(error) => {
                assert_eq!(expected["ok"], false, "throw case {config:?} must fail in TypeScript");
                assert_eq!(error.to_string(), expected["error"]["message"], "throw case {config:?}");
            }
        }
    }

    let mixed = [
        ("Authorization", "Bearer $PI_DIFF_RESOLVE_A"),
        ("X-Literal", "plain"),
        ("X-Missing", "$PI_DIFF_RESOLVE_ABSENT"),
    ];
    let all_missing = [("X-Missing", "$PI_DIFF_RESOLVE_ABSENT")];
    let header_cases = vec![
        HeaderCase {
            request: headers_request(next(), Some(&mixed), Some(&explicit), false),
            values: Some(&mixed),
            env: Some(&explicit),
            throw_on_error: false,
        },
        HeaderCase {
            request: headers_request(next(), None, Some(&explicit), false),
            values: None,
            env: Some(&explicit),
            throw_on_error: false,
        },
        HeaderCase {
            request: headers_request(next(), Some(&all_missing), Some(&explicit), false),
            values: Some(&all_missing),
            env: Some(&explicit),
            throw_on_error: false,
        },
        HeaderCase {
            request: headers_request(next(), Some(&mixed), Some(&explicit), true),
            values: Some(&mixed),
            env: Some(&explicit),
            throw_on_error: true,
        },
        HeaderCase {
            request: headers_request(next(), Some(&all_missing), Some(&explicit), true),
            values: Some(&all_missing),
            env: Some(&explicit),
            throw_on_error: true,
        },
    ];
    for case in &header_cases {
        let expected = oracle.request(&case.request);
        let values = case.values.map(env);
        let explicit_env = optional_env(case.env);
        let actual = if case.throw_on_error {
            resolve_headers_or_throw(values.as_ref(), &JsString::from("Provider"), explicit_env.as_ref())
                .map(json_headers)
                .map_err(|error| error.to_string())
        } else {
            Ok(json_headers(resolve_headers(values.as_ref(), explicit_env.as_ref())))
        };
        match actual {
            Ok(value) => assert_eq!(json!({"headers": value}), expected["output"], "headers case {values:?}"),
            Err(message) => {
                assert_eq!(expected["ok"], false, "headers case {values:?} must fail in TypeScript");
                assert_eq!(message, expected["error"]["message"], "headers case {values:?}");
            }
        }
    }
}
