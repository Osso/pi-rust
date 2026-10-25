use crate::shared_oracle as oracle;

use pi_coding_agent::core::tools::output_accumulator::{OutputAccumulator, OutputAccumulatorOptions};
use serde_json::{Value, json};
use std::ffi::OsString;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{Command, Output, Stdio};

fn events(output: &Output) -> Vec<String> {
    String::from_utf8(output.stdout.clone())
        .unwrap()
        .lines()
        .filter(|line| line.contains("\"event\""))
        .map(|line| {
            let frame: Value = serde_json::from_str(&line[line.find('{').unwrap()..]).expect("event frame");
            frame["event"].as_str().unwrap().to_owned()
        })
        .collect()
}

fn last_error(output: &Output) -> Value {
    let mut error = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| line.contains("\"event\""))
        .map(|line| serde_json::from_str::<Value>(&line[line.find('{').unwrap()..]).unwrap())
        .filter_map(|frame| frame.get("error").cloned())
        .fold(None, |_, error| Some(error))
        .expect("original native error frame");
    if let Some(path) = error["path"].as_str().map(str::to_owned) {
        assert!(path.contains("/pi-acc"), "{error}");
        let message = error["message"].as_str().unwrap();
        assert!(message.contains(&path));
        error["message"] = json!(message.replace(&path, "<artifact>"));
        error["path"] = json!("<artifact>");
    }
    error
}

fn compile_fault_fixture(root: &Path) -> OsString {
    let library = root.join("accumulator-fault.so");
    let compilation = Command::new("cc")
        .args(["-shared", "-fPIC", "-o"])
        .arg(&library)
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/support/bash_spill_failure.c"
        ))
        .arg("-ldl")
        .output()
        .unwrap();
    assert!(
        compilation.status.success(),
        "{}",
        String::from_utf8_lossy(&compilation.stderr)
    );
    library.into_os_string()
}

#[test]
fn actual_stream_fault_delivery_matches_listener_and_finish_boundaries() {
    let lock = oracle::read_source_lock();
    oracle::verify_source_head(&lock);
    let mut healthy = oracle::TsOracle::spawn(
        "coding-agent-output-accumulator-oracle.ts",
        "packages/coding-agent/src/core/tools/output-accumulator.ts",
    );
    let control = healthy.request(&json!({"id": 27008, "cases": [{"name": "healthy-spill", "maxBytes": 1,
        "chunks": [b"payload"], "finish": true, "persistIfTruncated": true}]}));
    assert_eq!(control["ok"], true, "{control}");
    assert_eq!(control["output"][0]["tempFileHex"], "7061796c6f6164");
    let fixture_root = tempfile::tempdir().unwrap();
    let library = compile_fault_fixture(fixture_root.path());
    let mut mismatches = Vec::new();
    for (fault, close) in [
        ("open", false),
        ("open", true),
        ("write", false),
        ("write", true),
        ("replay-write", false),
        ("replay-write", true),
        ("write-close", false),
        ("write-close", true),
        ("close", true),
    ] {
        let root = tempfile::tempdir().unwrap();
        let blocked = root.path().join("blocked");
        std::fs::create_dir(&blocked).unwrap();
        std::fs::set_permissions(&blocked, std::fs::Permissions::from_mode(0o500)).unwrap();
        let mut env = vec![
            ("TMPDIR", root.path().into()),
            ("PI_ACC_FAULT", fault.into()),
            ("PI_ACC_CLOSE", if close { "1" } else { "0" }.into()),
        ];
        if fault != "open" {
            env.extend([
                ("LD_PRELOAD", library.clone()),
                ("PI_BASH_SPILL_PATH", "/pi-acc".into()),
                (
                    "PI_BASH_SPILL_FAIL",
                    match fault {
                        "close" => "close",
                        "write-close" => "write-close",
                        _ => "write",
                    }
                    .into(),
                ),
            ]);
        }
        let expected = oracle::spawn_oracle_process(
            &lock,
            "output-accumulator-open-error-oracle.mts",
            "packages/coding-agent/src/core/tools/output-accumulator.ts",
            &env,
            Stdio::piped(),
        )
        .wait_with_output()
        .unwrap();
        let expected_events = events(&expected);
        let source_events = if fault == "close" {
            vec![
                "append-start",
                "append-returned",
                "close-start",
                "close-resolved",
                "second-close-resolved",
                "uncaught",
            ]
        } else if close {
            vec!["append-start", "append-returned", "close-start", "close-rejected"]
        } else {
            vec!["append-start", "append-returned", "uncaught"]
        };
        assert_eq!(
            expected_events,
            source_events,
            "{fault}: {}",
            String::from_utf8_lossy(&expected.stderr)
        );
        assert_eq!(
            expected.status.code(),
            Some(if close && fault != "close" { 0 } else { 1 })
        );
        let last: Value =
            serde_json::from_str(String::from_utf8_lossy(&expected.stdout).lines().last().unwrap()).unwrap();
        let code = match fault {
            "open" => "EACCES",
            "close" | "write-close" => "EBADF",
            _ => "EIO",
        };
        assert_eq!(last["error"]["code"], code, "{last}");
        let syscall = match fault {
            "replay-write" => "write",
            "write-close" => "close",
            _ => fault,
        };
        assert_eq!(last["error"]["syscall"], syscall, "{last}");
        let path = if fault == "open" {
            blocked.join("pi-acc-native.log")
        } else {
            root.path().join("pi-acc-native.log")
        };
        let actual = Command::new(std::env::current_exe().unwrap())
            .args([
                "--ignored",
                "--exact",
                "accumulator_stream_failure_child",
                "--nocapture",
            ])
            .envs(env.iter().map(|(key, value)| (key, value)))
            .env("PI_ACC_FAILURE_PATH", path)
            .output()
            .unwrap();
        let actual_events = events(&actual);
        if actual.status.code() == Some(1) {
            assert!(
                String::from_utf8_lossy(&actual.stderr).starts_with(&format!("Error: {code}:")),
                "{actual:?}"
            );
        }
        assert_eq!(
            pi_js_compat::js_json::JsValue::from(last_error(&actual)),
            pi_js_compat::js_json::JsValue::from(last_error(&expected)),
            "original {fault} error metadata"
        );
        if actual_events != expected_events || actual.status.code() != expected.status.code() {
            mismatches.push(
                json!({"fault": fault, "close": close, "actual": actual_events, "expected": expected_events,
                "actualExit": actual.status.code(), "expectedExit": expected.status.code()}),
            );
        }
    }
    assert!(mismatches.is_empty(), "{}", json!(mismatches));
}

#[tokio::test(flavor = "current_thread")]
#[ignore = "real isolated accumulator stream-error boundary"]
async fn accumulator_stream_failure_child() {
    let Some(path) = std::env::var_os("PI_ACC_FAILURE_PATH") else {
        return;
    };
    let fault = std::env::var("PI_ACC_FAULT").unwrap();
    pi_coding_agent::utils::uncaught::install_uncaught_handler();
    pi_coding_agent::utils::uncaught::set_process_crash_handler(Some(std::sync::Arc::new(|payload| {
        if let Some(error) = payload.downcast_ref::<pi_js_compat::node_error::NativeNodeError>() {
            println!("{}", json!({"event": "uncaught", "error": error.value()}));
        }
    })));
    let mut output = OutputAccumulator::new_with_temp_file_path(
        OutputAccumulatorOptions {
            max_bytes: if fault == "replay-write" { 4.0 } else { 1.0 },
            ..Default::default()
        },
        path.to_str().unwrap(),
    );
    println!("{}", json!({"event": "append-start"}));
    if fault == "replay-write" {
        output.append(b"ab").unwrap();
    }
    output.append(b"payload").unwrap();
    println!("{}", json!({"event": "append-returned"}));
    if std::env::var("PI_ACC_CLOSE").as_deref() == Ok("1") {
        println!("{}", json!({"event": "close-start"}));
        match output.close_temp_file() {
            Ok(()) => {
                println!("{}", json!({"event": "close-resolved"}));
                if fault == "close" {
                    output.close_temp_file().unwrap();
                    println!("{}", json!({"event": "second-close-resolved"}));
                }
            }
            Err(error) => println!("{}", json!({"event": "close-rejected", "error": error.value()})),
        }
    }
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    if std::env::var("PI_ACC_CLOSE").as_deref() != Ok("1") {
        panic!("expected an uncaught stream error before the bounded wait expired");
    }
}

// Shared support is compiled once per integration target.
#[path = "support/oracle.rs"]
pub mod shared_oracle;
