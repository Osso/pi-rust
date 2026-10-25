#[path = "../examples/extensions/auto_commit_on_exit.rs"]
mod example;
#[path = "support/extension_examples_e_git.rs"]
mod git_support;
use crate::shared_extension_examples_e as harness;
use serde_json::{Value, json};

fn assistant(content: Value) -> Value {
    json!({"role":"assistant","content":content,"api":"faux","provider":"faux","model":"faux-1","usage":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0,"totalTokens":0,"cost":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0,"total":0}},"stopReason":"stop","timestamp":0})
}

#[tokio::test]
async fn shutdown_commits_all_changes_using_latest_assistant_first_line() {
    if git_support::run_in_isolated_git_child("shutdown_commits_all_changes_using_latest_assistant_first_line") {
        return;
    }
    let shutdown = json!({"op":"event","event":{"type":"session_shutdown","reason":"quit"}});
    for text in [
        "short title\nsecond line",
        "",
        "\nignored line",
        &"a".repeat(50),
        &"b".repeat(51),
        &format!("{}🚀", "x".repeat(48)),
        &format!("{}🚀", "x".repeat(49)),
    ] {
        let result=harness::compare("auto-commit-on-exit.ts",harness::factory(example::auto_commit_on_exit_extension),json!({"git":true,"files":{"tracked.txt":"changed\n","new.txt":"new\n"},"steps":[
            {"op":"append","message":assistant(json!([{"type":"text","text":"older message"}]))},
            {"op":"append","message":assistant(json!([{"type":"thinking","thinking":"not commit text"},{"type":"text","text":text},{"type":"text","text":"later text block"}]))},
            {"op":"append","message":{"role":"user","content":"tail","timestamp":0}},
            shutdown,
            {"op":"snapshot","files":["tracked.txt","new.txt"]},
            shutdown
        ]})).await;
        assert_eq!(result["cases"][4]["result"]["status"], "");
        assert_eq!(result["cases"][5]["ui"], json!([]), "second shutdown is clean");
        if text == format!("{}🚀", "x".repeat(49)) {
            let commit = format!("[pi] {}�...", "x".repeat(49));
            assert_eq!(
                result["cases"][3]["ui"],
                json!([["notify", format!("Auto-committed: {commit}"), "info"]])
            );
            assert!(
                result["cases"][4]["result"]["messages"]
                    .as_str()
                    .unwrap()
                    .starts_with(&commit)
            );
        }
    }
}

#[tokio::test]
async fn no_repo_clean_repo_no_assistant_no_ui_and_commit_failure_match() {
    if git_support::run_in_isolated_git_child("no_repo_clean_repo_no_assistant_no_ui_and_commit_failure_match") {
        return;
    }
    let shutdown = json!({"op":"event","event":{"type":"session_shutdown","reason":"reload"}});
    let not_repo = harness::compare(
        "auto-commit-on-exit.ts",
        harness::factory(example::auto_commit_on_exit_extension),
        json!({"steps":[shutdown]}),
    )
    .await;
    assert_eq!(not_repo["cases"][0]["ui"], json!([]));
    for (files, has_ui, reject) in [
        (json!({}), true, false),
        (json!({"new.txt":"new"}), true, false),
        (json!({"new.txt":"new"}), false, false),
        (json!({"new.txt":"new"}), true, true),
    ] {
        let result = harness::compare(
            "auto-commit-on-exit.ts",
            harness::factory(example::auto_commit_on_exit_extension),
            json!({"git":true,"files":files,"rejectCommit":reject,"steps":[
                {"op":"event","hasUI":has_ui,"event":{"type":"session_shutdown","reason":"quit"}},
                {"op":"snapshot","files":["tracked.txt","new.txt"]}
            ]}),
        )
        .await;
        if reject {
            assert_eq!(result["cases"][1]["result"]["status"], "A  new.txt\n");
            assert_eq!(result["cases"][0]["ui"], json!([]));
        }
        if !has_ui {
            assert_eq!(result["cases"][0]["ui"], json!([]));
            assert!(
                result["cases"][1]["result"]["messages"]
                    .as_str()
                    .unwrap()
                    .starts_with("[pi] Work in progress")
            );
        }
    }
}

// Shared support is compiled once per integration target.
#[path = "support/extension_examples_e.rs"]
pub mod shared_extension_examples_e;
#[path = "support/extensions.rs"]
pub mod shared_extensions;
#[path = "support/oracle.rs"]
pub mod shared_oracle;
