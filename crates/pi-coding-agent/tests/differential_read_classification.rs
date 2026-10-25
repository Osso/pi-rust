use crate::shared_oracle as oracle;

use pi_coding_agent::core::extensions::types::ToolRenderContext;
use pi_coding_agent::core::tools::read;
use pi_coding_agent::modes::interactive::theme::{ColorMode, load_theme_from_path, set_theme_instance};
use pi_js_compat::js_json::{JsString, JsValue};
use serde_json::{Value, json};
use std::{process::Command, sync::Arc};

#[test]
fn read_classification_keeps_posix_labels_priority_and_path_resolution_errors() {
    let package = tempfile::tempdir().unwrap();
    let themes = package.path().join("dist/modes/interactive/theme");
    std::fs::create_dir_all(&themes).unwrap();
    for name in ["dark", "light"] {
        std::fs::copy(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("theme/{name}.json")),
            themes.join(format!("{name}.json")),
        )
        .unwrap();
    }
    let mut oracle = oracle::TsOracle::spawn_with_environment(
        "read-classification-oracle.mts",
        "packages/coding-agent/src/core/tools/read.ts",
        &[
            ("PI_PACKAGE_DIR", package.path().as_os_str().to_owned()),
            ("COLORTERM", "truecolor".into()),
        ],
    );
    let mut cases: Vec<Value> = [
        "README.md",
        "docs/guide.md",
        "examples/demo.rs",
        "docs\\guide.md",
        "docs/part\\name.md",
        "docs/AGENTS.md",
        "docs/SKILL.md",
        "",
        "../sibling/docs/guide.md",
    ]
    .into_iter()
    .map(|path| json!({"path":package.path().join(path),"cwd":"/repo"}))
    .collect();
    cases.push(json!({"path":"SKILL.md","cwd":"","deletedCwd":true}));
    cases.push(json!({"path":"file.txt","cwd":"","deletedCwd":true,"expanded":true,"hyperlinks":true}));
    cases.push(json!({"path":"/read.txt","cwd":"/repo","packageDir":"file://not-local/pkg"}));
    let mut mismatches = Vec::new();
    for case in cases {
        let expected: Value = oracle.request(&case);
        let output = Command::new(std::env::current_exe().unwrap())
            .args(["--ignored", "--exact", "classification_child", "--nocapture"])
            .env(
                "PI_PACKAGE_DIR",
                case.get("packageDir")
                    .and_then(Value::as_str)
                    .map(std::path::Path::new)
                    .unwrap_or(package.path()),
            )
            .env("PI_CLASSIFICATION_CASE", case.to_string())
            .output()
            .unwrap();
        if !output.status.success() {
            mismatches.push(format!(
                "{case}: native child exited {}: {}",
                output.status,
                String::from_utf8(output.stderr).unwrap()
            ));
            continue;
        }
        let stdout = String::from_utf8(output.stdout).unwrap();
        let actual: Value =
            serde_json::from_str(stdout.lines().find_map(|line| line.strip_prefix("RESULT:")).unwrap()).unwrap();
        if actual != expected {
            mismatches.push(format!("{case}: {actual}\nexpected {expected}"));
        }
    }
    assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
}

#[test]
#[ignore = "isolated config/deleted-cwd child"]
fn classification_child() {
    let case: Value = serde_json::from_str(&std::env::var("PI_CLASSIFICATION_CASE").unwrap()).unwrap();
    let theme_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("theme/dark.json");
    let theme = Arc::new(
        load_theme_from_path(
            &JsString::from(theme_path.to_str().unwrap()),
            Some(ColorMode::Truecolor),
        )
        .unwrap(),
    );
    set_theme_instance(Arc::clone(&theme));
    pi_tui::set_capabilities(pi_tui::TerminalCapabilities {
        images: None,
        true_color: true,
        hyperlinks: case["hyperlinks"].as_bool().unwrap_or(false),
    });
    if case["deletedCwd"].as_bool().unwrap_or(false) {
        let directory = tempfile::tempdir().unwrap();
        pi_js_compat::process_cwd::chdir(&pi_js_compat::node_fs::decode_node_path(std::ffi::OsStr::new(
            &(directory.path()),
        )))
        .unwrap();
        directory.close().unwrap();
    }
    let args = JsValue::from(json!({"path":case["path"]}));
    let context = ToolRenderContext {
        args: args.clone(),
        cwd: case["cwd"].as_str().unwrap().into(),
        expanded: case["expanded"].as_bool().unwrap_or(false),
        tool_call_id: "classification".into(),
        invalidate: Arc::new(|| {}),
        last_component: None,
        call_component: None,
        state: pi_coding_agent::core::extensions::types::ToolRenderState::new(),
        execution_started: false,
        args_complete: true,
        is_partial: false,
        show_images: true,
        is_error: false,
    };
    let output = match read::render_call(&args, &theme, &context) {
        Ok(component) => json!({"value":component.borrow_mut().render(100)}),
        Err(error) => json!({"error":error.message}),
    };
    println!("RESULT:{output}");
}

// Shared support is compiled once per integration target.
#[path = "support/oracle.rs"]
pub mod shared_oracle;
