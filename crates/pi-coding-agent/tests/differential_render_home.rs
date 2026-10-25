use crate::shared_oracle as oracle;

use pi_coding_agent::core::extensions::types::ToolRenderContext;
use pi_coding_agent::core::tools::{read, render_utils};
use pi_coding_agent::modes::interactive::components::tool_execution::{ToolExecutionComponent, ToolExecutionOptions};
use pi_coding_agent::modes::interactive::theme::{ColorMode, load_theme_from_path, set_theme_instance};
use pi_js_compat::js_json::{JsString, JsValue};
use pi_tui::Component;
use serde_json::{Value, json};
use std::{process::Command, sync::Arc};

#[test]
fn unset_home_and_failed_passwd_lookup_match_frozen_renderer_boundary() {
    let directory = tempfile::tempdir().unwrap();
    let library = directory.path().join("passwd-failure.so");
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/support/passwd_failure.c");
    let compiled = Command::new("cc")
        .args(["-shared", "-fPIC", "-o"])
        .arg(&library)
        .arg(source)
        .arg("-ldl")
        .output()
        .unwrap();
    assert!(compiled.status.success(), "{compiled:?}");
    let mut oracle = oracle::TsOracle::spawn_with_environment(
        "render-home-oracle.mts",
        "packages/coding-agent/src/core/tools/render-utils.ts",
        &[
            ("LD_PRELOAD", library.as_os_str().to_owned()),
            ("COLORTERM", "truecolor".into()),
        ],
    );
    let home = nix::unistd::User::from_uid(nix::unistd::Uid::effective())
        .unwrap()
        .unwrap()
        .dir;
    let path = home.join("fixture.txt").to_str().unwrap().to_owned();
    let mut mismatches = Vec::new();
    for (path, expanded) in [(&path[..], true), ("~/fixture.txt", false)] {
        for fail in [false, true] {
            let expected: Value = oracle.request(&json!({"path":path,"fail":fail,"expanded":expanded}));
            let output = Command::new(std::env::current_exe().unwrap())
                .args(["--ignored", "--exact", "render_home_child", "--nocapture"])
                .env_remove("HOME")
                .env("LD_PRELOAD", &library)
                .env("PI_TEST_FAIL_PASSWD", if fail { "1" } else { "0" })
                .env("PI_TEST_RENDER_PATH", path)
                .env("PI_TEST_RENDER_EXPANDED", expanded.to_string())
                .output()
                .unwrap();
            assert!(output.status.success(), "{output:?}");
            let stdout = String::from_utf8(output.stdout).unwrap();
            let actual: Value =
                serde_json::from_str(stdout.lines().find_map(|line| line.strip_prefix("RESULT:")).unwrap()).unwrap();
            if actual != expected {
                mismatches.push(format!("failed lookup {fail}: actual {actual}\nexpected {expected}"));
            }
        }
    }
    assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
}

#[test]
#[ignore = "isolated HOME/passwd child"]
fn render_home_child() {
    let path = std::env::var("PI_TEST_RENDER_PATH").unwrap();
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
        hyperlinks: false,
    });
    let args = JsValue::from(json!({"path":path}));
    let context = ToolRenderContext {
        args: args.clone(),
        tool_call_id: "home-read".into(),
        cwd: "/repo".into(),
        expanded: std::env::var("PI_TEST_RENDER_EXPANDED").unwrap().parse().unwrap(),
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
    let call = read::render_call(&args, &theme, &context);
    let component = ToolExecutionComponent::new(
        "read",
        "home-read",
        args,
        ToolExecutionOptions::default(),
        Some(read::create_read_tool_definition(
            "/repo",
            read::ReadToolOptions::default(),
        )),
        Arc::new(|| {}),
        "/repo",
    );
    let value = json!({
        "shorten": match render_utils::shorten_path(&JsString::from(path.clone())) { Ok(value) => json!({"value":value}), Err(error) => json!({"error":error.message}) },
        "normalized": path_outcome(pi_coding_agent::utils::paths::normalize_path(path.clone(), &Default::default())),
        "resolved": path_outcome(pi_coding_agent::utils::paths::resolve_path(path.clone(), "/repo", &Default::default())),
        "canonical": pi_coding_agent::utils::paths::canonicalize_path(path),
        "call": match call { Ok(call) => json!({"value":call.borrow_mut().render(80)}), Err(error) => json!({"error":error.message}) },
        "component":component.borrow_mut().render(80),
    });
    println!("RESULT:{value}");
}

fn path_outcome(result: Result<JsString, pi_coding_agent::utils::paths::PathError>) -> Value {
    match result {
        Ok(value) => json!({"value":value}),
        Err(error) => json!({"error":error.message()}),
    }
}

// Shared support is compiled once per integration target.
#[path = "support/oracle.rs"]
pub mod shared_oracle;
