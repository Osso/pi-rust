use crate::shared_oracle as oracle;
use pi_coding_agent::core::tools::bash::{
    BashDetachOptions, BashExecOptions, BashOperations, DetachedJobArtifacts, create_local_bash_operations,
};
use pi_js_compat::abort::AbortSignal;
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::{self, JsString};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

#[test]
fn durable_output_descriptor_closes_before_observation_and_overrides_spawn_validation() {
    let root = tempfile::tempdir().unwrap();
    let library = root.path().join("descriptor-failure.so");
    let output = Command::new("cc")
        .args(["-shared", "-fPIC", "-o"])
        .arg(&library)
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/support/bash_spill_failure.c"))
        .arg("-ldl")
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let mut source = oracle::TsOracle::spawn_with_environment(
        "bash-descriptor-oracle.ts",
        "packages/coding-agent/src/core/tools/bash.ts",
        &[
            ("LD_PRELOAD", library.as_os_str().to_owned()),
            ("PI_BASH_SPILL_FAIL", "close".into()),
            ("PI_BASH_SPILL_PATH", "/durable-output.log".into()),
        ],
    );
    let mut failures = Vec::new();
    for (index, command) in ["true", "nul\0command"].into_iter().enumerate() {
        let source_dir = root.path().join(format!("source-{index}"));
        std::fs::create_dir(&source_dir).unwrap();
        let expected=source.request(&json!({"id":index,"command":command,"cwd":root.path().to_str().unwrap(),"directory":source_dir.to_str().unwrap(),"outputPath":source_dir.join("durable-output.log").to_str().unwrap()}));
        assert_eq!(expected["ok"], json!(true), "{expected:?}");
        assert_eq!(
            expected["output"],
            json!({"result":{"ok":false,"error":"EBADF: bad file descriptor, close"},"artifactExists":true}),
            "{expected:?}"
        );
        let native_dir = root.path().join(format!("native-{index}"));
        std::fs::create_dir(&native_dir).unwrap();
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args(["--ignored", "--exact", "durable_descriptor_child", "--nocapture"])
            .current_dir(root.path())
            .env("PI_DESCRIPTOR_DIR", &native_dir)
            .env("PI_DESCRIPTOR_NUL", if index == 1 { "1" } else { "0" })
            .env("LD_PRELOAD", &library)
            .env("PI_BASH_SPILL_FAIL", "close")
            .env("PI_BASH_SPILL_PATH", "/durable-output.log")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        while child.try_wait().unwrap().is_none() {
            if Instant::now() >= deadline {
                child.kill().unwrap();
                let output = child.wait_with_output().unwrap();
                panic!(
                    "descriptor child timed out: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let output = child.wait_with_output().unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        let text = String::from_utf8(output.stdout).unwrap();
        let line = text
            .lines()
            .find_map(|line| line.strip_prefix("DESCRIPTOR_RESULT:"))
            .unwrap();
        let actual = js_json::parse_lossless(line).unwrap();
        if actual != expected["output"] {
            failures.push(json!({"case":index,"actual":actual,"expected":expected["output"]}));
        }
    }
    assert!(failures.is_empty(), "{}", js_json::stringify_lossless(&json!(failures)));
}
#[test]
#[ignore = "isolated real output-descriptor syscall fault"]
fn durable_descriptor_child() {
    let directory = std::env::var("PI_DESCRIPTOR_DIR").unwrap();
    let cwd = pi_js_compat::process_cwd::cwd().unwrap();
    let command = if std::env::var("PI_DESCRIPTOR_NUL").unwrap() == "1" {
        "nul\0command"
    } else {
        "true"
    };
    let result = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(create_local_bash_operations(None).exec(
            &command.into(),
            &cwd,
            BashExecOptions {
                on_data: Arc::new(|_| Ok(())),
                signal: None,
                timeout: None,
                env: None,
                detach: Some(BashDetachOptions {
                    artifacts: Some(DetachedJobArtifacts {
                        directory: directory.as_str().into(),
                        output_path: JsString::from(directory.clone()) + "/durable-output.log",
                    }),
                    job_id: None,
                    signal: AbortSignal::default(),
                    adopt: None,
                }),
            },
        ));
    let result = match result {
        Ok(value) => json!({"ok":true,"value":{"exitCode":value.exit_code}}),
        Err(error) => json!({"ok":false,"error":error.message}),
    };
    println!(
        "DESCRIPTOR_RESULT:{}",
        js_json::stringify_lossless(&json!({"result":result,"artifactExists":Path::new(&directory).exists()}))
    );
}

// Shared support is compiled once per integration target.
#[path = "support/oracle.rs"]
pub mod shared_oracle;
