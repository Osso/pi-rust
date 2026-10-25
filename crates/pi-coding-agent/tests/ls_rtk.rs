use std::sync::Arc;

use pi_agent_core::agent::{AgentTool, AgentToolResult};
use pi_ai::types::TextOrImageContent;
use pi_coding_agent::core::tools::ls::LsTool;
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::{JsString, JsValue};

use crate::shared_executable_script as executable_script;

const MODE_VAR: &str = "PI_LS_RTK_MODE";
const DIR_VAR: &str = "PI_LS_RTK_DIR";

fn execute(tool: &LsTool, params: JsValue) -> Result<AgentToolResult, JsString> {
    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    runtime
        .block_on(tool.execute("call".to_owned(), params, None, Arc::new(|_result| Ok(())), None))
        .map_err(|error| error.message)
}

fn text(result: &AgentToolResult) -> JsString {
    JsString::join(
        result.content.iter().filter_map(|entry| match entry {
            TextOrImageContent::Text(text) => Some(&text.text),
            TextOrImageContent::Image(_) => None,
        }),
        "\n",
    )
}

fn fake_rtk(directory: &std::path::Path, body: &str) {
    let path = directory.join("rtk");
    executable_script::write_executable_script(&path, &format!("#!/bin/sh\n{body}\n"));
}

#[test]
fn rtk_fast_path_limit_errors_and_fallback() {
    match std::env::var(MODE_VAR).as_deref() {
        Ok("rtk") => run_rtk_cases(),
        Ok("fallback") => run_fallback_case(),
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

    let rtk = std::process::Command::new(&executable)
        .args(["--exact", "rtk_fast_path_limit_errors_and_fallback", "--nocapture"])
        .env(MODE_VAR, "rtk")
        .env(DIR_VAR, &bin)
        .env("PATH", format!("{}:{original_path}", bin.display()))
        .status()
        .expect("rtk child");
    assert!(rtk.success(), "rtk child failed");

    let fallback = std::process::Command::new(&executable)
        .args(["--exact", "rtk_fast_path_limit_errors_and_fallback", "--nocapture"])
        .env(MODE_VAR, "fallback")
        .env(DIR_VAR, &empty)
        .env("PATH", empty.display().to_string())
        .status()
        .expect("fallback child");
    assert!(fallback.success(), "fallback child failed");
}

fn run_rtk_cases() {
    let bin = std::path::PathBuf::from(std::env::var(DIR_VAR).expect("rtk dir"));
    let temp = tempfile::tempdir().expect("temp dir");
    let tool = LsTool::new(temp.path().to_str().expect("utf-8 temp path"));

    fake_rtk(&bin, "printf 'alpha.txt\\nbeta/\\n'");
    let result = execute(&tool, json!({})).expect("rtk result");
    assert_eq!(text(&result), "alpha.txt\nbeta/");

    let result = execute(&tool, json!({"limit": 1})).expect("rtk limit result");
    assert_eq!(
        text(&result),
        "alpha.txt\n\n[1 entries limit reached. Use limit=2 for more]"
    );

    fake_rtk(&bin, "printf 'ignored\\n' >&2\nexit 3");
    let error = execute(&tool, json!({})).expect_err("rtk failure");
    assert_eq!(error, "ignored");
}

fn run_fallback_case() {
    let temp = tempfile::tempdir().expect("temp dir");
    let empty = std::path::PathBuf::from(std::env::var(DIR_VAR).expect("empty dir"));
    std::fs::create_dir(temp.path().join("sub")).expect("sub dir");
    std::fs::write(temp.path().join("real.txt"), "x").expect("real file");
    let tool = LsTool::new(temp.path().to_str().expect("utf-8 temp path"));
    let result = execute(&tool, json!({})).expect("fallback result");
    assert_eq!(text(&result), "real.txt\nsub/");
    assert!(empty.is_dir());
}

// Shared support is compiled once per integration target.
#[path = "support/executable_script.rs"]
pub mod shared_executable_script;
