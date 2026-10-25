//! Frozen/default native provisioning against a private fake authsudo, never a live broker.
#[path = "support/env_probe.rs"]
mod env_probe;
#[path = "support/executable_script.rs"]
mod executable_script;
#[path = "support/bundled_extension_load.rs"]
mod load_support;
#[path = "support/oracle.rs"]
mod oracle;
#[path = "support/bundled_extensions.rs"]
mod support;

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use pi_coding_agent::bundled_extensions::ask_secret::{AskSecretToolOptions, create_ask_secret_tool_definition};
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::{JsString, JsValue};

fn params() -> JsValue {
    json!({"record":"browser/private-fixture","fields":[
        {"type":"email","name":"Username","selector":"#login"},
        {"type":"password","name":"Password","selector":"#password"}
    ]})
}

fn inputs() -> Vec<JsString> {
    vec![
        "fixture-user".into(),
        JsString::from_utf16(vec![0xd800, 0x61, 0x0a, 0x62]),
    ]
}

fn write_fake_authsudo(directory: &Path) -> OsString {
    executable_script::write_executable_script(
        &directory.join("authsudo"),
        r#"#!/usr/bin/python3
import json, os, pathlib, signal, sys, time
capture = pathlib.Path(os.environ["PI_SECRET_CAPTURE"])
raw = sys.stdin.buffer.read().decode("utf8")
capture.write_text(json.dumps({"argv":sys.argv[1:],"stdin":raw}))
if os.environ["PI_SECRET_MODE"] == "abort":
    def terminate(_signal, _frame):
        capture.with_suffix(".term").write_text("SIGTERM")
        sys.exit(0)
    signal.signal(signal.SIGTERM, terminate)
    capture.with_suffix(".ready").write_text("ready")
    while True:
        time.sleep(0.01)
sys.exit(7 if os.environ["PI_SECRET_MODE"] == "failure" else 0)
"#,
    );
    let mut paths = vec![directory.to_path_buf()];
    paths.extend(std::env::split_paths(&std::env::var_os("PATH").unwrap()));
    std::env::join_paths(paths).unwrap()
}

fn environment(path: &OsString, capture: &Path, mode: &str, result: &Path) -> Vec<(&'static str, OsString)> {
    vec![
        ("PATH", path.clone()),
        ("PI_SECRET_CAPTURE", capture.as_os_str().to_owned()),
        ("PI_SECRET_MODE", mode.into()),
        ("PI_SECRET_RESULT", result.as_os_str().to_owned()),
    ]
}

#[test]
fn default_secret_process_protocol_failure_and_abort_match_frozen() {
    for mode in ["success", "failure", "abort"] {
        let directory = tempfile::tempdir().unwrap();
        let path = write_fake_authsudo(directory.path());
        let capture = directory.path().join("source.json");
        let result = directory.path().join("result.json");
        let env = environment(&path, &capture, mode, &result);
        let mut oracle = oracle::TsOracle::spawn_with_environment(
            "bundled-extensions-oracle.ts",
            "packages/coding-agent/extensions/ask-secret/src/index.ts",
            &env,
        );
        let mut step =
            json!({"op":"tool","name":"ask_secret","params":params(),"mode":"tui","hasUI":true,"inputs":inputs()});
        if mode == "abort" {
            step["abortOnReady"] = json!(capture.with_extension("ready").to_str().unwrap());
        }
        let response = oracle.request(&json!({"id":mode,"extension":"ask-secret","useDefaultProvision":true,"steps":[{"op":"registrations"},step]}));
        assert_eq!(response["ok"], json!(true));
        let expected = &response["output"]["steps"][1];
        let frozen_wire = std::fs::read_to_string(&capture).unwrap();
        let wire = pi_js_compat::js_json::parse_lossless(&frozen_wire).unwrap();
        assert_eq!(
            wire["argv"],
            json!([
                "-u",
                "secrets-broker",
                "/usr/bin/secrets-broker-admin",
                "provision-browser",
                "--record",
                "browser/private-fixture",
                "--fields-json",
                pi_js_compat::js_json::stringify_lossless(&params()["fields"])
            ])
        );
        assert_eq!(
            wire["stdin"],
            json!(pi_js_compat::js_json::stringify_lossless(&json!(inputs())))
        );
        if mode == "abort" {
            assert_eq!(
                std::fs::read_to_string(capture.with_extension("term")).unwrap(),
                "SIGTERM"
            );
        }

        let native_capture = directory.path().join("native.json");
        env_probe::run_probe(
            "ask_secret_default_process_probe",
            &environment(&path, &native_capture, mode, &result),
        );
        let native = pi_js_compat::js_json::parse_lossless(&std::fs::read_to_string(&result).unwrap()).unwrap();
        assert_eq!(native["result"], expected["result"], "{mode} result");
        assert_eq!(native["error"], expected["error"], "{mode} error");
        assert_eq!(native["calls"], expected["calls"], "{mode} calls");
        assert_eq!(native["registrations"], response["output"]["steps"][0]["result"]);
        assert_eq!(
            std::fs::read_to_string(&native_capture).unwrap(),
            frozen_wire,
            "{mode} wire"
        );
        if mode == "abort" {
            assert_eq!(
                std::fs::read_to_string(native_capture.with_extension("term")).unwrap(),
                "SIGTERM"
            );
        }
    }
}

#[tokio::test]
#[ignore = "child probe invoked by default_secret_process_protocol_failure_and_abort_match_frozen"]
async fn ask_secret_default_process_probe() {
    if !env_probe::in_probe() {
        return;
    }
    let ui = Arc::new(support::RecordingUi::default());
    ui.inputs.lock().unwrap().extend(inputs().into_iter().map(Some));
    let harness = support::Harness::load("/tmp", ui, |api| {
        pi_coding_agent::bundled_extensions::ask_secret::ask_secret_extension(api)
    })
    .await;
    let abort = if std::env::var("PI_SECRET_MODE").unwrap() == "abort" {
        Some(pi_js_compat::abort::AbortController::new())
    } else {
        None
    };
    let signal = abort.as_ref().map(|controller| controller.signal());
    let watch = abort.map(|controller| {
        tokio::spawn(async move {
            let ready = PathBuf::from(std::env::var("PI_SECRET_CAPTURE").unwrap()).with_extension("ready");
            let deadline = Instant::now() + Duration::from_secs(10);
            while !ready.exists() {
                if Instant::now() >= deadline {
                    controller.abort(None);
                    panic!("fake provisioner did not become ready");
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            controller.abort(None);
        })
    });
    let tool = create_ask_secret_tool_definition(AskSecretToolOptions::default());
    let outcome = (tool.execute)("call-1".to_owned(), params(), signal, None, Some(harness.context())).await;
    if let Some(watch) = watch {
        watch.await.unwrap();
    }
    let mut value = match outcome {
        Ok(result) => json!({"result":result,"error":null}),
        Err(error) => {
            let error = pi_ai::request::CallbackError::from(error);
            json!({"result":null,"error":{"name":error.name,"message":error.message}})
        }
    };
    value["calls"] = support::calls_json(&harness.take_calls());
    value["registrations"] = harness.registrations();
    std::fs::write(
        std::env::var("PI_SECRET_RESULT").unwrap(),
        pi_js_compat::js_json::stringify_lossless(&value),
    )
    .unwrap();
}
