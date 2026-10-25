#[path = "../examples/extensions/git_checkpoint.rs"]
mod example;
#[path = "support/extension_examples_e_git.rs"]
mod git_support;
use crate::shared_extension_examples_e as harness;
use serde_json::{Value, json};

fn tool_result() -> Value {
    json!({"op":"tool_result","event":{"type":"tool_result","toolName":"read","toolCallId":"fixture","input":{"path":"tracked.txt"},"content":[],"isError":false}})
}
fn turn() -> Value {
    json!({"op":"event","event":{"type":"turn_start","turnIndex":0,"timestamp":0}})
}
fn fork(entry: &str, choice: Option<&str>, has_ui: bool) -> Value {
    json!({"op":"event","hasUI":has_ui,"selected":choice,"event":{"type":"session_before_fork","entryId":entry,"position":"before"}})
}
fn reset() -> Value {
    json!({"op":"git","args":["reset","--hard","-q","HEAD"]})
}
fn snapshot() -> Value {
    json!({"op":"snapshot","files":["tracked.txt"]})
}

#[tokio::test]
async fn checkpoints_restore_only_on_matching_entry_and_yes_with_ui() {
    if git_support::run_in_isolated_git_child("checkpoints_restore_only_on_matching_entry_and_yes_with_ui") {
        return;
    }
    for (choice, has_ui, restored) in [
        (None, true, false),
        (Some("No, keep current code"), true, false),
        (Some("Yes, restore code to that point"), true, true),
        (Some("Yes please"), true, true),
        (Some("yes"), true, false),
        (Some("Yes, restore code to that point"), false, false),
    ] {
        let result = harness::compare(
            "git-checkpoint.ts",
            harness::factory(example::git_checkpoint_extension),
            json!({"git":true,"files":{"tracked.txt":"checkpoint\n"},"steps":[
                tool_result(),turn(),
                {"op":"append","message":{"role":"user","content":"first","timestamp":0}},
                tool_result(),turn(),reset(),
                {"op":"append","message":{"role":"user","content":"later","timestamp":0}},
                fork("$leaf",choice,has_ui),
                fork("$0",choice,has_ui),snapshot()
            ]}),
        )
        .await;
        assert_eq!(result["cases"][7]["ui"], json!([]), "different entry has no checkpoint");
        assert_eq!(
            result["cases"][9]["result"]["files"]["tracked.txt"],
            if restored { "checkpoint\n" } else { "initial\n" }
        );
        if restored {
            assert_eq!(
                result["cases"][8]["ui"][1],
                json!(["notify", "Code restored to checkpoint", "info"])
            );
        }
    }
}

#[tokio::test]
async fn clean_stash_missing_repo_agent_end_clear_and_failed_apply_preserve_source_behavior() {
    if git_support::run_in_isolated_git_child(
        "clean_stash_missing_repo_agent_end_clear_and_failed_apply_preserve_source_behavior",
    ) {
        return;
    }
    let result=harness::compare("git-checkpoint.ts",harness::factory(example::git_checkpoint_extension),json!({"steps":[
        {"op":"append","message":{"role":"user","content":"first","timestamp":0}},tool_result(),turn(),fork("$0",Some("Yes"),true)
    ]})).await;
    assert_eq!(result["cases"][3]["ui"], json!([]));
    let result=harness::compare("git-checkpoint.ts",harness::factory(example::git_checkpoint_extension),json!({"git":true,"steps":[
        {"op":"append","message":{"role":"user","content":"first","timestamp":0}},tool_result(),turn(),fork("$0",Some("Yes"),true),
        {"op":"write","files":{"tracked.txt":"checkpoint\n"}},turn(),reset(),
        {"op":"event","event":{"type":"agent_end","messages":[]}},fork("$0",Some("Yes"),true),snapshot(),
        {"op":"write","files":{"tracked.txt":"new checkpoint\n"}},turn(),reset(),fork("$0",Some("Yes"),true),snapshot(),
        {"op":"write","files":{"tracked.txt":"conflicting unsaved edits\n"}},fork("$0",Some("Yes"),true),snapshot()
    ]})).await;
    assert_eq!(result["cases"][3]["ui"], json!([]), "clean stash has no ref");
    assert_eq!(result["cases"][8]["ui"], json!([]), "agent_end clears map");
    assert_eq!(
        result["cases"][14]["result"]["files"]["tracked.txt"], "new checkpoint\n",
        "agent_end retains current entry id"
    );
    assert_eq!(
        result["cases"][17]["result"]["files"]["tracked.txt"],
        "conflicting unsaved edits\n"
    );
    assert_eq!(
        result["cases"][16]["ui"][1],
        json!(["notify", "Code restored to checkpoint", "info"]),
        "source notifies even if git stash apply fails"
    );
}

// Shared support is compiled once per integration target.
#[path = "support/extension_examples_e.rs"]
pub mod shared_extension_examples_e;
#[path = "support/extensions.rs"]
pub mod shared_extensions;
#[path = "support/oracle.rs"]
pub mod shared_oracle;
