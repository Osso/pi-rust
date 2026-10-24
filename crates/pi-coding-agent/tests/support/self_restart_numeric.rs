use super::*;

pub(super) fn number_flags(code: f64) -> JsValue {
    object([
        ("negativeZero", JsValue::Bool(code == 0.0 && code.is_sign_negative())),
        ("nan", JsValue::Bool(code.is_nan())),
        ("positiveInfinity", JsValue::Bool(code == f64::INFINITY)),
        ("negativeInfinity", JsValue::Bool(code == f64::NEG_INFINITY)),
    ])
}
pub(super) fn fixture_exit_code(request: &JsValue) -> Option<f64> {
    match request.get("resultCode") {
        Some(JsValue::Null) => None,
        Some(value) => Some(pi_js_compat::number::parse_number(
            &value.as_string().unwrap().to_utf8().unwrap(),
        )),
        None => (request.get("operation") != Some(&text("helper-null"))).then_some(17.0),
    }
}
const CODES: &[&str] = &[
    "-0",
    "17.5",
    "NaN",
    "Infinity",
    "-Infinity",
    "2147483648",
    "-2147483649",
    "4294967295",
    "-4294967297",
    "9007199254740991",
    "9007199254740992",
    "-9007199254740992",
    "1e100",
    "-1",
    "17",
];

#[tokio::test]
async fn callback_numeric_domain_matches_actual_frozen_module() {
    let mut source = oracle::TsOracle::spawn("self-restart-process-oracle.mts", "packages/coding-agent/src");
    for code in CODES {
        for operation in ["spawn-return", "helper-exit"] {
            let mut request = fixture_request(operation, "native", 0);
            request.as_object_mut().unwrap().insert("resultCode".into(), text(code));
            let expected = source.request(&request);
            let actual = if operation == "spawn-return" {
                run_restart(&request).await
            } else {
                run_helper(&request).await
            };
            assert_same_wire(&actual, &expected, &format!("{operation}/{code}"));
        }
    }
}

const PRIVATE_EXIT: &str = "PI_PRIVATE_RESTART_DEFAULT_EXIT";
#[test]
fn default_exit_child() {
    let Some(input) = std::env::var_os(PRIVATE_EXIT) else {
        return;
    };
    let input = std::path::PathBuf::from(input);
    let request = pi_js_compat::js_json::parse_lossless(std::fs::read_to_string(&input).unwrap()).unwrap();
    let code = fixture_exit_code(&request).unwrap();
    let dependencies = SelfRestartDependencies {
        execve: None,
        spawn_self_restart: Some(Arc::new(move |_, _| Box::pin(async move { Ok(code) }))),
        ..Default::default()
    };
    let executor = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let error = executor
        .block_on(restart_current_process(restart_request(&request), &dependencies))
        .unwrap_err();
    let wire = object([("calls", JsValue::Array(vec![])), ("error", error_wire(&error))]);
    std::fs::write(
        input.parent().unwrap().join("error.json"),
        pi_js_compat::js_json::stringify_lossless(&wire),
    )
    .unwrap();
    std::process::exit(0);
}

#[test]
fn default_exit_numeric_validation_and_status_match_frozen() {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let temp = tempfile::tempdir().unwrap();
    let lock = oracle::read_source_lock();
    oracle::verify_source_head(&lock);
    for (index, code) in CODES.iter().enumerate() {
        let root = temp.path().join(index.to_string());
        std::fs::create_dir(&root).unwrap();
        let mut request = fixture_request("default-exit", "native", 0);
        request.as_object_mut().unwrap().insert("resultCode".into(), text(code));
        let mut source = oracle::spawn_oracle_process(
            &lock,
            "self-restart-process-oracle.mts",
            "packages/coding-agent/src",
            &[],
            Stdio::piped(),
        );
        writeln!(
            source.stdin.take().unwrap(),
            "{}",
            pi_js_compat::js_json::stringify_lossless(&request)
        )
        .unwrap();
        let expected = source.wait_with_output().unwrap();
        let input = root.join("input.json");
        std::fs::write(&input, pi_js_compat::js_json::stringify_lossless(&request)).unwrap();
        let actual = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "numeric::default_exit_child", "--nocapture"])
            .env(PRIVATE_EXIT, &input)
            .stdin(Stdio::null())
            .output()
            .unwrap();
        assert_eq!(
            actual.status.code(),
            expected.status.code(),
            "exit status/{code}: {actual:?}, {expected:?}"
        );
        let output = String::from_utf8(expected.stdout).unwrap();
        if output.trim().is_empty() {
            assert!(!root.join("error.json").exists(), "valid exit must not reject/{code}");
        } else {
            let expected = pi_js_compat::js_json::parse_lossless(output.trim()).unwrap();
            let actual =
                pi_js_compat::js_json::parse_lossless(std::fs::read_to_string(root.join("error.json")).unwrap())
                    .unwrap();
            assert_same_wire(&actual, &expected, &format!("exit validation/{code}"));
        }
    }
}
