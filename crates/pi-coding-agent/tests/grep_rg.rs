use std::sync::Arc;

use pi_agent_core::agent::AgentTool;
use pi_coding_agent::core::tools::grep::GrepTool;
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::JsString;

use crate::shared_executable_script as executable_script;

const MODE_VAR: &str = "PI_GREP_TEST_MODE";
const DIR_VAR: &str = "PI_GREP_TEST_DIR";

#[test]
fn grep_uses_real_ripgrep_argv_and_reports_missing_binary() {
    match std::env::var(MODE_VAR).as_deref() {
        Ok("fake") => run_fake_rg(),
        Ok("missing") => run_missing_rg(),
        _ => spawn_children(),
    }
}

fn spawn_children() {
    let temp = tempfile::tempdir().expect("temp dir");
    let bin = temp.path().join("bin");
    let empty = temp.path().join("empty");
    std::fs::create_dir(&bin).expect("bin dir");
    std::fs::create_dir(&empty).expect("empty dir");
    let original_path = std::env::var("PATH").expect("PATH");
    let executable = std::env::current_exe().expect("test executable");

    let fake = std::process::Command::new(&executable)
        .args([
            "--exact",
            "grep_uses_real_ripgrep_argv_and_reports_missing_binary",
            "--nocapture",
        ])
        .env(MODE_VAR, "fake")
        .env(DIR_VAR, &bin)
        .env("PATH", format!("{}:{original_path}", bin.display()))
        .status()
        .expect("fake rg child");
    assert!(fake.success(), "fake rg child failed");

    let missing = std::process::Command::new(&executable)
        .args([
            "--exact",
            "grep_uses_real_ripgrep_argv_and_reports_missing_binary",
            "--nocapture",
        ])
        .env(MODE_VAR, "missing")
        .env(DIR_VAR, &empty)
        .env("PATH", empty.display().to_string())
        .env("PI_OFFLINE", "1")
        .status()
        .expect("missing rg child");
    assert!(missing.success(), "missing rg child failed");
}

fn write_fake_rg(directory: &std::path::Path, args_path: &std::path::Path) {
    let path = directory.join("rg");
    let script = format!(
        "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then\n  printf 'ripgrep 15.2.0\\n'\n  exit 0\nfi\nprintf '%s' \"$*\" > '{}'\nsearched=\"\"\nfor arg in \"$@\"; do searched=\"$arg\"; done\nprintf '{{\"type\":\"match\",\"data\":{{\"path\":{{\"text\":\"%s\"}},\"lines\":{{\"text\":\"match\\\\n\"}},\"line_number\":1}}}}\\n' \"$searched\"\n",
        args_path.display()
    );
    executable_script::write_executable_script(&path, &script);
}

fn run_fake_rg() {
    let bin = std::path::PathBuf::from(std::env::var(DIR_VAR).expect("bin dir"));
    let temp = tempfile::tempdir().expect("temp dir");
    let args_path = temp.path().join("args.txt");
    let searched = temp.path().join("searched.txt");
    std::fs::write(&searched, "match\n").expect("searched file");
    write_fake_rg(&bin, &args_path);

    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    let tool = GrepTool::new(temp.path().to_str().expect("utf-8 temp path"));
    let result = runtime
        .block_on(tool.execute(
            "grep-argv".to_owned(),
            json!({"pattern": "match", "path": searched.to_str().expect("utf-8 test path")}),
            None,
            Arc::new(|_result| Ok(())),
            None,
        ))
        .expect("grep result");
    let parts = result
        .content
        .iter()
        .filter_map(|entry| match entry {
            pi_ai::types::TextOrImageContent::Text(text) => Some(text.text.clone()),
            pi_ai::types::TextOrImageContent::Image(_) => None,
        })
        .collect::<Vec<_>>();
    let text = JsString::join(&parts, "\n");
    assert!(text.contains("searched.txt:1: match"), "{text:?}");
    let args = std::fs::read_to_string(&args_path).expect("args file");
    assert!(args.contains("--max-columns 10240"), "{args}");
    assert!(args.contains("--json"), "{args}");
    assert!(args.contains("--hidden"), "{args}");
}

fn run_missing_rg() {
    let temp = tempfile::tempdir().expect("temp dir");
    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    let tool = GrepTool::new(temp.path().to_str().expect("utf-8 temp path"));
    let error = runtime
        .block_on(tool.execute(
            "grep-missing".to_owned(),
            json!({"pattern": "match"}),
            None,
            Arc::new(|_result| Ok(())),
            None,
        ))
        .expect_err("missing rg");
    assert_eq!(error.message, "ripgrep (rg) is not available on PATH");
}

// Shared support is compiled once per integration target.
#[path = "support/executable_script.rs"]
pub mod shared_executable_script;
