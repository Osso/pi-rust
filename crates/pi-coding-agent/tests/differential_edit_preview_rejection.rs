use crate::shared_oracle as oracle;
pub use crate::shared_render_owner as render_owner;

use pi_coding_agent::core::extensions::types::{ToolRenderContext, ToolRenderState};
use pi_coding_agent::core::tools::edit::render_call;
use pi_coding_agent::modes::interactive::theme::{ColorMode, load_theme_from_path};
use pi_coding_agent::utils::uncaught;
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::{self, JsString, JsValue};
use std::process::Command;
use std::sync::Arc;

#[test]
#[ignore = "invoked by process-boundary differential"]
fn edit_preview_rejection_child() {
    let Ok(path) = std::env::var("PI_EDIT_PREVIEW_REJECTION_PATH") else {
        return;
    };
    uncaught::install_uncaught_handler();
    let default_handler = std::env::var_os("PI_EDIT_PREVIEW_DEFAULT_HANDLER").is_some();
    if !default_handler {
        uncaught::set_process_crash_handler(Some(Arc::new(|payload| {
            let native = payload
                .downcast_ref::<pi_ai::request::CallbackError>()
                .and_then(|error| error.thrown_value().as_error())
                .or_else(|| {
                    payload
                        .downcast_ref::<pi_js_compat::node_error::NativeNodeError>()
                        .cloned()
                })
                .expect("original native preview rejection");
            let record = json!({"unhandled":true,"name":native.name,"message":native.message,
            "constructor":native.constructor_class().name(),"string":native.string_value()});
            println!("PREVIEW_REJECTION:{}", js_json::stringify_lossless(&record));
            std::process::exit(19);
        })));
    }
    let theme_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("theme/dark.json");
    let theme = load_theme_from_path(
        &JsString::from(theme_path.to_str().unwrap()),
        Some(ColorMode::Truecolor),
    )
    .unwrap();
    let args = json!({"path":path,"edits":[{"oldText":"before","newText":"after"}]});
    let state = ToolRenderState::new();
    let owner = std::env::var_os("PI_EDIT_PREVIEW_OWNER").map(|_| render_owner::RenderOwner::new());
    if let Some(owner) = &owner {
        owner.bind(&state);
    }
    let context = ToolRenderContext {
        args: args.clone(),
        tool_call_id: "preview-rejection".into(),
        invalidate: Arc::new(|| {}),
        last_component: None,
        call_component: None,
        state,
        cwd: "/repo".into(),
        execution_started: false,
        args_complete: true,
        is_partial: false,
        expanded: false,
        show_images: false,
        is_error: false,
    };
    let _ = render_call(&args, &theme, &context);
    println!("SYNC_TURN_END");
    if let Some(owner) = &owner {
        owner.yield_for(std::time::Duration::from_millis(700));
    } else {
        std::thread::sleep(std::time::Duration::from_millis(700));
    }
    println!("PREVIEW_REJECTION:{{\"unhandled\":false}}");
}

#[test]
fn rejected_preview_promise_reaches_uncaught_boundary_with_original_native_error() {
    let mut source = oracle::TsOracle::spawn(
        "edit-preview-rejection-oracle.ts",
        "packages/coding-agent/src/core/tools/edit.ts",
    );
    for path in ["file:///tmp/%", "file://remote/path"] {
        let expected: JsValue = source.request(&json!({"id":path,"cases":[{"path":path}]}));
        assert_eq!(expected["ok"], JsValue::Bool(true), "{expected:?}");
        let output = Command::new(std::env::current_exe().unwrap())
            .args(["--ignored", "--exact", "edit_preview_rejection_child", "--nocapture"])
            .env("PI_EDIT_PREVIEW_REJECTION_PATH", path)
            .output()
            .unwrap();
        let stdout = String::from_utf8(output.stdout).unwrap();
        let record = stdout
            .lines()
            .find_map(|line| line.strip_prefix("PREVIEW_REJECTION:"))
            .expect("native child record");
        let actual = js_json::parse_lossless(record).unwrap();
        assert_eq!(
            actual,
            expected["output"][0],
            "preview path {path}; stderr {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            output.status.code(),
            Some(19),
            "preview rejection must reach process handler"
        );
    }
}

#[test]
fn default_preview_rejection_crashes_after_the_sync_turn_with_node_stderr() {
    let source_lock = oracle::read_source_lock();
    oracle::verify_source_head(&source_lock);
    for path in ["file:///tmp/%", "file://remote/path"] {
        let expected = oracle::spawn_oracle_process(
            &source_lock,
            "edit-preview-rejection-process-oracle.mts",
            "packages/coding-agent/src/core/tools/edit.ts",
            &[("PI_EDIT_PREVIEW_REJECTION_PATH", path.into())],
            std::process::Stdio::piped(),
        )
        .wait_with_output()
        .unwrap();
        assert_eq!(expected.status.code(), Some(1), "frozen default handler");
        assert!(String::from_utf8_lossy(&expected.stdout).contains("SYNC_TURN_END"));
        let source_stderr = String::from_utf8(expected.stderr).unwrap();
        let headline = source_stderr
            .lines()
            .find(|line| line.starts_with("URIError: ") || line.starts_with("TypeError ["))
            .expect("Node error headline");
        for owner in [false, true] {
            let mut command = Command::new(std::env::current_exe().unwrap());
            command
                .args(["--ignored", "--exact", "edit_preview_rejection_child", "--nocapture"])
                .env("PI_EDIT_PREVIEW_REJECTION_PATH", path)
                .env("PI_EDIT_PREVIEW_DEFAULT_HANDLER", "1");
            if owner {
                command.env("PI_EDIT_PREVIEW_OWNER", "1");
            }
            let actual = command.output().unwrap();
            assert_eq!(actual.status.code(), Some(1), "default rejection, owner={owner}");
            assert!(
                String::from_utf8_lossy(&actual.stdout).contains("SYNC_TURN_END"),
                "must finish synchronous turn, owner={owner}"
            );
            assert!(
                String::from_utf8_lossy(&actual.stderr)
                    .lines()
                    .any(|line| line == headline),
                "owner={owner}: {} != {headline}",
                String::from_utf8_lossy(&actual.stderr)
            );
        }
    }
}

// Shared support is compiled once per integration target.
#[path = "support/oracle.rs"]
pub mod shared_oracle;
#[path = "support/render_owner.rs"]
pub mod shared_render_owner;
