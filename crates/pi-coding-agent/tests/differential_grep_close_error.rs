use crate::shared_executable_script as executable_script;
use crate::shared_oracle as oracle;
use pi_agent_core::agent::AgentTool;
use pi_ai::request::BoxFuture;
use pi_coding_agent::core::tools::grep::{GrepOperations, GrepTool, GrepToolOptions};
use pi_coding_agent::utils::uncaught;
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::{self, JsString, JsValue};
use std::{io, path::Path, process::Command, sync::Arc};
struct Operations;
impl GrepOperations for Operations {
    fn is_directory<'a>(&'a self, _: &'a JsString) -> BoxFuture<'a, io::Result<bool>> {
        Box::pin(async { Ok(true) })
    }
    fn read_file<'a>(&'a self, _: &'a JsString) -> BoxFuture<'a, io::Result<JsString>> {
        Box::pin(async { Ok("unused".into()) })
    }
}
fn fake_rg(root: &Path) -> String {
    let bin = root.join("bin");
    std::fs::create_dir(&bin).unwrap();
    let node = Command::new("node").args(["-p", "process.execPath"]).output().unwrap();
    assert!(node.status.success());
    let node = String::from_utf8(node.stdout).unwrap();
    let event = r#"{"type":"match","data":{"path":{"text":"relative.txt"},"line_number":1,"lines":{"text":"inline"}}}"#;
    let script = format!(
        "#!{}\nif(process.argv[2]==='--version')process.exit(0);process.stdout.write('{event}\\n');",
        node.trim()
    );
    executable_script::write_executable_script(&bin.join("rg"), &script);
    format!("{}:{}", bin.display(), std::env::var("PATH").unwrap())
}
fn run_native_child(root: &Path, path: &str, capture: bool) -> std::process::Output {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args(["--ignored", "--exact", "grep_close_error_child", "--nocapture"])
        .env("PI_GREP_CLOSE_ROOT", root)
        .env("PATH", path);
    if capture {
        command.env("PI_GREP_CLOSE_CAPTURE", "1");
    }
    command.output().unwrap()
}
#[test]
fn deleted_cwd_close_callback_retains_uncaught_native_identity_and_process_exit() {
    let root = tempfile::tempdir().unwrap();
    let path = fake_rg(root.path());
    let mut source = oracle::TsOracle::spawn_with_environment(
        "grep-close-error-oracle.ts",
        "packages/coding-agent/src/core/tools/grep.ts",
        &[("PATH", path.clone().into())],
    );
    let expected: JsValue = source.request(&json!({"id":1,"cwd":root.path()}));
    assert_eq!(expected["ok"], JsValue::Bool(true), "{expected:?}");
    assert_eq!(expected["output"]["kind"], json!("unhandledRejection"));
    assert_eq!(expected["output"]["settled"], JsValue::Bool(false));
    let captured = run_native_child(root.path(), &path, true);
    let stdout = String::from_utf8(captured.stdout).unwrap();
    let record = stdout
        .lines()
        .find_map(|line| line.strip_prefix("GREP_CLOSE_ERROR:"))
        .expect("native uncaught record");
    assert_eq!(captured.status.code(), Some(19));
    assert_eq!(js_json::parse_lossless(record).unwrap(), expected["output"]);
    assert!(!stdout.contains("TOOL_SETTLED"));
    let lock = oracle::read_source_lock();
    oracle::verify_source_head(&lock);
    let frozen = oracle::spawn_oracle_process(
        &lock,
        "grep-close-error-process-oracle.mts",
        "packages/coding-agent/src/core/tools/grep.ts",
        &[
            ("PATH", path.clone().into()),
            ("PI_GREP_CLOSE_ROOT", root.path().as_os_str().to_owned()),
        ],
        std::process::Stdio::piped(),
    )
    .wait_with_output()
    .unwrap();
    let native = run_native_child(root.path(), &path, false);
    assert_eq!(frozen.status.code(), Some(1));
    assert_eq!(native.status.code(), frozen.status.code());
    let headline = format!(
        "Error: {}",
        expected["output"]["error"]["message"]
            .as_string()
            .unwrap()
            .to_utf8()
            .unwrap()
    );
    assert!(
        String::from_utf8_lossy(&frozen.stderr)
            .lines()
            .any(|line| line == headline),
        "{}",
        String::from_utf8_lossy(&frozen.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&native.stderr), format!("{headline}\n"));
    assert!(!String::from_utf8_lossy(&native.stdout).contains("TOOL_SETTLED"));
}
#[test]
#[ignore = "isolated actual process-uncaught boundary"]
fn grep_close_error_child() {
    let root = std::env::var("PI_GREP_CLOSE_ROOT").unwrap();
    uncaught::install_uncaught_handler();
    if std::env::var_os("PI_GREP_CLOSE_CAPTURE").is_some() {
        uncaught::set_process_crash_handler(Some(Arc::new(|payload| {
            let record = match payload.downcast_ref::<pi_js_compat::node_error::NativeNodeError>() {
                Some(native) => json!({"kind":"unhandledRejection","error":native.value(),"settled":false}),
                None => json!({"kind":"non-native"}),
            };
            println!("GREP_CLOSE_ERROR:{}", js_json::stringify_lossless(&record));
            std::process::exit(19);
        })));
    }
    let deleted = Path::new(&root).join("native-deleted");
    std::fs::create_dir(&deleted).unwrap();
    std::env::set_current_dir(&deleted).unwrap();
    std::fs::remove_dir(deleted).unwrap();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let tool = GrepTool::with_options(
        root,
        GrepToolOptions {
            operations: Some(Arc::new(Operations)),
        },
    );
    let _ = runtime.block_on(tool.execute(
        "grep-close-error".into(),
        json!({"pattern":"x"}),
        None,
        Arc::new(|_| Ok(())),
        None,
    ));
    println!("TOOL_SETTLED");
}

// Shared support is compiled once per integration target.
#[path = "support/executable_script.rs"]
pub mod shared_executable_script;
#[path = "support/oracle.rs"]
pub mod shared_oracle;
