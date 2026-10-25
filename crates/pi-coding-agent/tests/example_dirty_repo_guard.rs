#[path = "../examples/extensions/dirty_repo_guard.rs"]
mod example;
use crate::shared_extensions as extension_support;
#[path = "support/extension_examples_a.rs"]
mod harness;
use crate::shared_oracle as oracle;
use serde_json::json;

fn git(cwd: &std::path::Path, args: &[&str]) {
    let result = std::process::Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .unwrap();
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
}

#[tokio::test]
async fn actual_private_git_clean_dirty_and_non_repository_match_ts() {
    let temp = tempfile::tempdir().unwrap();
    let steps = json!([
        {"op":"emit","event":{"type":"session_before_switch","reason":"new"}},
        {"op":"emit","event":{"type":"session_before_switch","reason":"resume"}},
        {"op":"emit","event":{"type":"session_before_fork","entryId":"abc","position":"before"}}
    ]);
    for state in ["not-repo", "clean", "dirty"] {
        if state == "clean" {
            git(temp.path(), &["init", "--quiet"]);
        }
        if state == "dirty" {
            std::fs::write(temp.path().join("one.txt"), "one").unwrap();
            std::fs::write(temp.path().join("two.txt"), "two").unwrap();
        }
        for mode in ["tui", "json"] {
            let actual = harness::compare("dirty-repo-guard", harness::factory(example::dirty_repo_guard_extension), json!({"cwd":temp.path(),"mode":mode,"noUi":mode == "json","dialogs":[null,"Yes, proceed anyway","No, let me commit first"],"steps":steps})).await;
            if state == "dirty" {
                assert!(
                    actual["trace"]
                        .as_array()
                        .unwrap()
                        .contains(&json!(["result",{"cancel":true}]))
                );
                if mode == "tui" {
                    assert_eq!(
                        actual["trace"][0][1],
                        "You have 2 uncommitted file(s). new session anyway?"
                    );
                }
            } else {
                assert_eq!(
                    actual["trace"],
                    json!([["result", null], ["result", null], ["result", null]])
                );
            }
        }
    }
}

// Shared support is compiled once per integration target.
#[path = "support/extensions.rs"]
pub mod shared_extensions;
#[path = "support/oracle.rs"]
pub mod shared_oracle;
