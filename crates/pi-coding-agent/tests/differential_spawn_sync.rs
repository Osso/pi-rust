#[path = "support/oracle.rs"]
mod oracle;

use pi_coding_agent::utils::child_process::{SpawnSyncOptions, spawn_sync};
use pi_js_compat::js_json::{JsString, JsValue, to_value};
use serde_json::json;
use std::path::{Path, PathBuf};
use std::time::Duration;

#[test]
fn explicit_sync_limits_and_result_errors_match_node() {
    let state = tempfile::tempdir().unwrap();
    std::fs::write(state.path().join("config.json"), "{}").unwrap();
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/spawn_sync.py");
    let marker = state.path().join("signal");
    let mut source = oracle::TsOracle::spawn_with_environment(
        "clipboard-image-module-oracle.mts",
        "packages/coding-agent/src",
        &[("PI_CLIPBOARD_MATRIX_STATE", state.path().as_os_str().to_owned())],
    );
    let mut failures = Vec::new();
    for (case, options) in [
        ("out", json!({})),
        ("signal", json!({})),
        ("stdout", json!({"maxBuffer":12})),
        ("stderr", json!({"maxBuffer":12})),
        ("combined", json!({"maxBuffer":12})),
        ("idle", json!({"timeout":250})),
        ("closed", json!({"timeout":250})),
        ("idle", json!({"timeout":250,"killSignal":"SIGKILL"})),
        ("out", json!({"maxBuffer":0,"timeout":0})),
        ("missing", json!({})),
        ("null-arg", json!({})),
        ("out", json!({"cwd":state.path().join("missing-directory")})),
        ("out", json!({"cwd":"/tmp/invalid\u{0}cwd"})),
    ] {
        let program: JsString = if case == "missing" {
            state.path().join("missing-program").to_str().unwrap().into()
        } else {
            "/usr/bin/python3".into()
        };
        let arguments: Vec<JsString> = if case == "null-arg" {
            vec![JsString::from_utf16(vec![0x61, 0, 0x62])]
        } else {
            vec![
                fixture.to_str().unwrap().into(),
                case.into(),
                marker.to_str().unwrap().into(),
            ]
        };
        let mut request =
            JsValue::from(json!({"operation":"sync","bin":"","temporary":state.path(),"options":options}));
        request
            .as_object_mut()
            .unwrap()
            .insert("program".into(), JsValue::String(program.clone()));
        request
            .as_object_mut()
            .unwrap()
            .insert("arguments".into(), to_value(&arguments).unwrap());
        let expected = source.request(&request);
        let expected_signal = std::fs::read_to_string(&marker).unwrap_or_default();
        if marker.exists() {
            std::fs::remove_file(&marker).unwrap();
        }
        let cwd: Option<PathBuf> = options["cwd"].as_str().map(PathBuf::from);
        let actual = spawn_sync(
            &program,
            &arguments,
            SpawnSyncOptions {
                cwd: cwd.as_deref(),
                timeout: options["timeout"].as_u64().map(Duration::from_millis),
                max_buffer: options["maxBuffer"].as_u64().map(|value| value as usize),
                kill_signal: if options["killSignal"] == "SIGKILL" {
                    rustix::process::Signal::KILL
                } else {
                    rustix::process::Signal::TERM
                },
            },
        );
        let actual = match actual {
            Ok(output) => JsValue::from(
                json!({"pidPresent":output.pid>0,"status":output.status,"signal":output.signal,"stdout":output.stdout,"stderr":output.stderr,"error":output.error.map(|error|error.value().to_serde().unwrap())}),
            ),
            Err(error) => {
                let mut value = pi_js_compat::js_json::JsObject::new();
                value.insert("error".into(), error.value());
                JsValue::Object(value)
            }
        };
        let actual_signal = std::fs::read_to_string(&marker).unwrap_or_default();
        if marker.exists() {
            std::fs::remove_file(&marker).unwrap();
        }
        if actual != expected {
            failures.push(format!("{case}/{options}: actual={actual:?}, expected={expected:?}"));
        }
        if actual_signal != expected_signal {
            failures.push(format!(
                "{case}/{options}: signal actual={actual_signal:?}, expected={expected_signal:?}"
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
