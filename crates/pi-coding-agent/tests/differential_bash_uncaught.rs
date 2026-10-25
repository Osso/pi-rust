#![cfg(target_os = "linux")]
use crate::shared_oracle as oracle;
use pi_coding_agent::core::tools::bash::{
    BashDetachOptions, BashExecOptions, BashOperations, DetachedJobArtifacts, create_local_bash_operations,
};
use pi_coding_agent::utils::uncaught;
use pi_js_compat::abort::AbortController;
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::{self, JsString, JsValue};
use std::{path::Path, process::Command, sync::Arc};
const MARKER: &str = "__PI_BASH_READ_FAILURE__";

// Frozen bash.ts stream listeners and durable interval have no error catcher.
// Real OS read failures must reach the process hook with the native error intact.
#[test]
fn local_pipe_and_durable_poll_failures_remain_uncaught_native_errors() {
    let root = tempfile::tempdir().unwrap();
    let library = root.path().join("read-failure.so");
    let compiler = Command::new("cc")
        .args(["-shared", "-fPIC", "-o"])
        .arg(&library)
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/support/bash_read_failure.c"))
        .arg("-ldl")
        .output()
        .unwrap();
    assert!(
        compiler.status.success(),
        "{}",
        String::from_utf8_lossy(&compiler.stderr)
    );
    let cases = ["stdout", "stderr", "poll", "prepoll", "detachpoll"];
    let expected: Vec<JsValue> = cases
        .iter()
        .map(|mode| {
            let mut source = oracle::TsOracle::spawn_with_environment(
                "bash-local-uncaught-oracle.ts",
                "packages/coding-agent/src/core/tools/bash.ts",
                &[
                    ("LD_PRELOAD", library.as_os_str().to_owned()),
                    ("PI_GREP_READ_FAILURE_MARKER", MARKER.into()),
                ],
            );
            let response: JsValue =
                source.request(&json!({"id":28056,"cwd":root.path().to_str().unwrap(),"cases":[mode]}));
            assert_eq!(response["ok"], JsValue::Bool(true), "{response:?}");
            if *mode == "prepoll" {
                assert_eq!(
                    response["output"][0]["ok"],
                    JsValue::Bool(false),
                    "pre-aborted Promise setup rejects: {response:?}"
                );
            } else {
                assert!(
                    response["output"][0].get("uncaught").is_some(),
                    "frozen must observe the native uncaught boundary: {response:?}"
                );
            }
            response["output"][0].clone()
        })
        .collect();
    let mut failures = Vec::new();
    for (index, mode) in cases.iter().enumerate() {
        let output = Command::new(std::env::current_exe().unwrap())
            .args(["--ignored", "--exact", "bash_uncaught_child", "--nocapture"])
            .env("LD_PRELOAD", &library)
            .env("PI_GREP_READ_FAILURE_MARKER", MARKER)
            .env("PI_BASH_UNCAUGHT_MODE", mode)
            .env("PI_BASH_UNCAUGHT_CWD", root.path())
            .env(
                "PI_BASH_FAIL_OPEN_PATH",
                root.path().join(format!("uncaught-{mode}/output.log")),
            )
            .output()
            .unwrap();
        let stdout = String::from_utf8(output.stdout).unwrap();
        let record = stdout
            .lines()
            .find_map(|line| line.strip_prefix("BASH_UNCAUGHT:"))
            .unwrap_or_else(|| panic!("missing record {stdout}: {}", String::from_utf8_lossy(&output.stderr)));
        let actual = js_json::parse_lossless(record).unwrap();
        let expected_status = if *mode == "prepoll" { 0 } else { 19 };
        if actual != expected[index] || output.status.code() != Some(expected_status) {
            failures
                .push(json!({"mode":mode,"status":output.status.code(),"actual":actual,"expected":expected[index]}));
        }
    }
    assert!(failures.is_empty(), "{}", js_json::stringify_lossless(&json!(failures)));
}
#[test]
#[ignore = "isolated real process-hook read failure"]
fn bash_uncaught_child() {
    uncaught::install_uncaught_handler();
    uncaught::set_process_crash_handler(Some(Arc::new(|payload| {
        let native = payload
            .downcast_ref::<pi_js_compat::node_error::NativeNodeError>()
            .cloned()
            .or_else(|| {
                let callback = payload.downcast_ref::<pi_js_compat::callback_error::CallbackError>()?;
                match callback.thrown_value() {
                    pi_js_compat::abort::AbortReason::Error(native) => Some(native),
                    _ => None,
                }
            });
        let record = match native {
            Some(native) => {
                json!({"uncaught":native.value(),"constructor":native.constructor_class().name(),"string":native.string_value()})
            }
            None => json!({"nonNative":true}),
        };
        println!("BASH_UNCAUGHT:{}", js_json::stringify_lossless(&record));
        std::process::exit(19);
    })));
    let mode = std::env::var("PI_BASH_UNCAUGHT_MODE").unwrap();
    let cwd = std::env::var("PI_BASH_UNCAUGHT_CWD").unwrap();
    let directory = Path::new(&cwd).join(format!("uncaught-{mode}"));
    let durable = matches!(mode.as_str(), "poll" | "prepoll" | "detachpoll");
    let artifacts = if durable {
        std::fs::create_dir(&directory).unwrap();
        Some(DetachedJobArtifacts {
            directory: directory.to_str().unwrap().into(),
            output_path: directory.join("output.log").to_str().unwrap().into(),
        })
    } else {
        None
    };
    let command: JsString = if mode == "poll" {
        format!("rm '{}/output.log'; sleep 1.3", directory.display()).into()
    } else if durable {
        "sleep 1.3".into()
    } else {
        format!("printf {MARKER} {}", if mode == "stderr" { ">&2" } else { "" }).into()
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let controller = AbortController::new();
    if mode == "prepoll" {
        controller.abort(None);
    } else if mode == "detachpoll" {
        let controller = controller.clone();
        runtime.spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            controller.abort(None);
        });
    }
    let result = runtime.block_on(create_local_bash_operations(None).exec(
        &command,
        &cwd.into(),
        BashExecOptions {
            on_data: Arc::new(|_| Ok(())),
            signal: None,
            timeout: None,
            env: None,
            detach: artifacts.map(|artifacts| BashDetachOptions {
                artifacts: Some(artifacts),
                job_id: None,
                signal: controller.signal(),
                adopt: None,
            }),
        },
    ));
    let record = match result {
        Ok(value) => json!({"ok":true,"value":{"exitCode":value.exit_code}}),
        Err(error) => json!({"ok":false,"error":error.message}),
    };
    println!("BASH_UNCAUGHT:{}", js_json::stringify_lossless(&record));
}

// Shared support is compiled once per integration target.
#[path = "support/oracle.rs"]
pub mod shared_oracle;
