#[path = "../examples/extensions/input_transform_streaming.rs"]
mod example;
use crate::shared_extensions as extension_support;
use crate::shared_oracle as oracle;
#[path = "support/extension_examples_b.rs"]
mod support;
use pi_coding_agent::core::extensions::*;
use serde_json::json;
use std::sync::Arc;
fn factory() -> ExtensionFactory {
    Arc::new(|api| {
        let result = example::input_transform_streaming_extension(api);
        Box::pin(async move { result })
    })
}
fn git(cwd: &std::path::Path, args: &[&str]) {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
}
#[tokio::test]
async fn prepends_real_private_git_stat_except_steering_or_no_match() {
    let temp = tempfile::tempdir().unwrap();
    git(temp.path(), &["init", "--quiet"]);
    let file = temp.path().join("fixture.txt");
    std::fs::write(&file, "one\ntwo\n").unwrap();
    git(temp.path(), &["add", "fixture.txt"]);
    std::fs::write(&file, "one\nchanged\nthree\n").unwrap();
    let image = json!({"type":"image","data":"AA==","mimeType":"image/png"});
    let mut cases = Vec::new();
    for (text, behavior) in [
        ("show changes", Some("steer")),
        ("hello", None),
        ("show changes", None),
        ("DIFF", Some("followUp")),
        ("émodifiedé", None),
        ("unchanged", None),
        ("diffs", None),
        ("change", None),
        ("diff", None),
    ] {
        cases.push(
            json!({"event":"input","text":text,"source":"interactive","streamingBehavior":behavior,"images":[image]}),
        );
    }
    let actual = support::compare(
        "input-transform-streaming",
        factory(),
        json!({"cwd":temp.path(),"cases":cases}),
    )
    .await;
    assert_eq!(actual["cases"][0]["result"], json!({"action":"continue"}));
    let transformed = &actual["cases"][2]["result"];
    assert_eq!(transformed["action"], "transform");
    assert!(
        transformed["text"]
            .as_str()
            .unwrap()
            .contains("Current uncommitted changes:\n```\nfixture.txt |")
    );
    assert_eq!(transformed["images"], json!([image]));
}
#[tokio::test]
async fn empty_stat_or_failed_git_keeps_input_unchanged() {
    let temp = tempfile::tempdir().unwrap();
    let request = json!({"cwd":temp.path(),"cases":[{"event":"input","text":"diff","source":"extension"}]});
    let failed = support::compare("input-transform-streaming", factory(), request.clone()).await;
    assert_eq!(failed["cases"][0]["result"], json!({"action":"continue"}));
    git(temp.path(), &["init", "--quiet"]);
    let empty = support::compare("input-transform-streaming", factory(), request).await;
    assert_eq!(empty["cases"][0]["result"], json!({"action":"continue"}));
}

// Shared support is compiled once per integration target.
#[path = "support/extensions.rs"]
pub mod shared_extensions;
#[path = "support/oracle.rs"]
pub mod shared_oracle;
#[path = "support/scripted_registry.rs"]
pub mod shared_scripted_registry;
#[path = "support/ui_thread.rs"]
pub mod shared_ui_thread;
