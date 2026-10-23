use pi_coding_agent::core::system_prompt::{BuildSystemPromptOptions, build_system_prompt_with_clock};
use pi_js_compat::{
    js_json,
    js_json::{JsString, JsValue},
};
use std::io::{Read, Write};

pub(crate) const OUTPUT_PREFIX: &str = "PI_RUST_SYSTEM_PROMPT_OUTPUT:";

pub(crate) fn run_child_probe() {
    let mut input = String::new();
    std::io::stdin()
        .read_to_string(&mut input)
        .expect("system prompt probe stdin");
    let request = js_json::parse_lossless(&input).expect("probe JSON");
    let options: BuildSystemPromptOptions =
        js_json::from_value(request.get("options").expect("options")).expect("native options");
    let now_millis = request.get("nowMillis").and_then(JsValue::as_f64).expect("fixed clock");
    let prompt = build_system_prompt_with_clock(&options, || now_millis);
    println!(
        "{OUTPUT_PREFIX}{}",
        js_json::stringify_lossless(&JsValue::String(prompt))
    );
}

pub(crate) fn spawn_child_probe(package_dir: &str, options: &JsValue, now_millis: f64, time_zone: &str) -> JsString {
    use std::process::{Command, Stdio};
    let mut child = Command::new(std::env::current_exe().expect("test executable"))
        .env_clear()
        .env("PI_PACKAGE_DIR", package_dir)
        .env("HOME", "/home/tester")
        .env("TZ", time_zone)
        .args([
            "--ignored",
            "--exact",
            "system_prompt_probe_child",
            "--nocapture",
            "--test-threads=1",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("system prompt child probe");
    {
        let stdin = child.stdin.as_mut().expect("probe stdin");
        let request = js_json!({"options":options,"nowMillis":now_millis});
        writeln!(stdin, "{}", js_json::stringify_lossless(&request)).expect("probe input");
    }
    let output = child.wait_with_output().expect("probe output");
    assert!(output.status.success(), "probe failed: {:?}", output.status);
    let stdout = String::from_utf8(output.stdout).expect("probe UTF-8 transport");
    let payload = stdout
        .split(OUTPUT_PREFIX)
        .nth(1)
        .and_then(|rest| rest.split('\n').next())
        .unwrap_or_else(|| panic!("probe payload missing: {stdout:?}"));
    js_json::parse_lossless(payload)
        .expect("probe JSON payload")
        .as_string()
        .expect("prompt string")
        .clone()
}
