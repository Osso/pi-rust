#![cfg(target_os = "linux")]
use crate::shared_executable_script as executable_script;
use crate::shared_oracle as oracle;
use pi_agent_core::agent::AgentTool;
use pi_ai::request::BoxFuture;
use pi_coding_agent::core::tools::grep::{GrepOperations, GrepTool, GrepToolOptions};
use pi_coding_agent::utils::uncaught;
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::{self, JsString, JsValue};
use std::{io, path::Path, process::Command, sync::Arc};
const MARKER: &str = "__PI_GREP_READ_FAILURE__";
struct Operations;
impl GrepOperations for Operations {
    fn is_directory<'a>(&'a self, _: &'a JsString) -> BoxFuture<'a, io::Result<bool>> {
        Box::pin(async { Ok(true) })
    }
    fn read_file<'a>(&'a self, _: &'a JsString) -> BoxFuture<'a, io::Result<JsString>> {
        Box::pin(async { Ok("unused".into()) })
    }
}
fn prepare_fixture(root: &Path) -> (String, std::path::PathBuf) {
    let library = root.join("read-failure.so");
    let compiler = Command::new("cc")
        .args(["-shared", "-fPIC", "-o"])
        .arg(&library)
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/support/grep_read_failure.c"))
        .arg("-ldl")
        .output()
        .unwrap();
    assert!(
        compiler.status.success(),
        "{}",
        String::from_utf8_lossy(&compiler.stderr)
    );
    let bin = root.join("bin");
    std::fs::create_dir(&bin).unwrap();
    let node = Command::new("node").args(["-p", "process.execPath"]).output().unwrap();
    assert!(node.status.success());
    let node = String::from_utf8(node.stdout).unwrap();
    let script = r#"if(process.argv[2]==='--version')process.exit(0);const fs=require('node:fs');const p=JSON.parse(process.env.PI_FAKE_RG_PROTOCOL);fs.writeSync(1,p.stdout||'');fs.writeSync(2,p.stderr||'');process.exit(0);"#;
    executable_script::write_executable_script(&bin.join("rg"), &format!("#!{}\n{script}", node.trim()));
    (format!("{}:{}", bin.display(), std::env::var("PATH").unwrap()), library)
}
#[test]
fn grep_pipe_read_errors_retain_uncaught_native_error_and_process_boundary() {
    let root = tempfile::tempdir().unwrap();
    let (path, library) = prepare_fixture(root.path());
    let cases = vec![json!({"stdout":MARKER}), json!({"stderr":MARKER})];
    let mut source = oracle::TsOracle::spawn_with_environment(
        "grep-protocol-oracle.ts",
        "packages/coding-agent/src/core/tools/grep.ts",
        &[
            ("PATH", path.clone().into()),
            ("LD_PRELOAD", library.as_os_str().to_owned()),
            ("PI_GREP_READ_FAILURE_MARKER", MARKER.into()),
        ],
    );
    let expected: JsValue = source.request(&json!({"id":1,"cases":cases}));
    assert_eq!(expected["ok"], JsValue::Bool(true), "{expected:?}");
    assert_eq!(expected["output"].as_array().unwrap().len(), cases.len());
    let mut failures = Vec::new();
    for (index, case) in cases.iter().enumerate() {
        let output = Command::new(std::env::current_exe().unwrap())
            .args(["--ignored", "--exact", "grep_pipe_error_child", "--nocapture"])
            .env("PATH", &path)
            .env("LD_PRELOAD", &library)
            .env("PI_GREP_READ_FAILURE_MARKER", MARKER)
            .env("PI_FAKE_RG_PROTOCOL", js_json::stringify_lossless(case))
            .output()
            .unwrap();
        let stdout = String::from_utf8(output.stdout).unwrap();
        let record = stdout
            .lines()
            .find_map(|line| line.strip_prefix("GREP_PIPE_ERROR:"))
            .unwrap_or_else(|| {
                panic!(
                    "missing native record: {stdout}; {}",
                    String::from_utf8_lossy(&output.stderr)
                )
            });
        let actual = js_json::parse_lossless(record).unwrap();
        if actual != expected["output"][index]["result"] || output.status.code() != Some(19) {
            failures.push(format!(
                "pipe{index} status{:?}: {} != {}",
                output.status.code(),
                record,
                js_json::stringify_lossless(&expected["output"][index]["result"])
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
#[test]
#[ignore = "isolated actual process-hook/pipe read failure"]
fn grep_pipe_error_child() {
    uncaught::install_uncaught_handler();
    uncaught::set_process_crash_handler(Some(Arc::new(|payload| {
        let record = match payload.downcast_ref::<pi_js_compat::node_error::NativeNodeError>() {
            Some(native) => {
                json!({"uncaught":native.value(),"constructor":native.constructor_class().name(),"string":native.string_value()})
            }
            None => json!({"nonNative":true}),
        };
        println!("GREP_PIPE_ERROR:{}", js_json::stringify_lossless(&record));
        std::process::exit(19);
    })));
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let tool = GrepTool::with_options(
        "/repo",
        GrepToolOptions {
            operations: Some(Arc::new(Operations)),
        },
    );
    let record = match runtime.block_on(tool.execute(
        "grep-protocol".into(),
        json!({"pattern":"x","path":"search"}),
        None,
        Arc::new(|_| Ok(())),
        None,
    )) {
        Ok(value) => json!({"ok":true,"value":js_json::to_value(&value).unwrap()}),
        Err(error) => json!({"ok":false,"error":error.message}),
    };
    println!("GREP_PIPE_ERROR:{}", js_json::stringify_lossless(&record));
}

// Shared support is compiled once per integration target.
#[path = "support/executable_script.rs"]
pub mod shared_executable_script;
#[path = "support/oracle.rs"]
pub mod shared_oracle;
