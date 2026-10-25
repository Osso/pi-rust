#[path = "../examples/extensions/git_merge_and_resolve.rs"]
mod example;
use crate::shared_extensions as extension_support;
#[path = "support/extension_examples_e_git.rs"]
mod git_support;
#[path = "support/extension_examples_a.rs"]
mod harness;
use crate::shared_oracle as oracle;
use serde_json::{Value, json};
use std::path::Path;

fn git(cwd: &Path, args: &[&str]) -> String {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}
fn setup(root: &Path, scenario: &str) -> std::path::PathBuf {
    let cwd = root.join("work");
    std::fs::create_dir(&cwd).unwrap();
    if scenario == "not-repo" {
        return cwd;
    }
    git(&cwd, &["init", "-q", "-b", "main"]);
    git(&cwd, &["config", "user.name", "Fixture"]);
    git(&cwd, &["config", "user.email", "fixture@example.invalid"]);
    std::fs::write(cwd.join("file.txt"), "initial\n").unwrap();
    git(&cwd, &["add", "."]);
    git(&cwd, &["commit", "-qm", "initial"]);
    if scenario == "dirty" {
        std::fs::write(cwd.join("file.txt"), "dirty").unwrap();
        return cwd;
    }
    if scenario == "no-upstream" {
        return cwd;
    }
    let remote = root.join("remote");
    git(
        root,
        &["clone", "-q", "--bare", cwd.to_str().unwrap(), remote.to_str().unwrap()],
    );
    git(&cwd, &["remote", "add", "origin", remote.to_str().unwrap()]);
    git(&cwd, &["fetch", "-q", "origin"]);
    git(&cwd, &["branch", "--set-upstream-to=origin/main"]);
    if scenario == "fetch-failure" {
        git(
            &cwd,
            &["remote", "set-url", "origin", root.join("missing").to_str().unwrap()],
        );
        return cwd;
    }
    if scenario == "clean" {
        return cwd;
    }
    git(&cwd, &["checkout", "-qb", "incoming"]);
    let theirs = if scenario == "empty-conflict" { "" } else { "theirs\n" };
    std::fs::write(cwd.join("file.txt"), theirs).unwrap();
    git(&cwd, &["commit", "-qam", "incoming"]);
    git(&cwd, &["push", "-q", "origin", "HEAD:main"]);
    git(&cwd, &["checkout", "-q", "main"]);
    let ours = if scenario == "multi-conflict" {
        "ours\nline two\n"
    } else {
        "ours\n"
    };
    std::fs::write(cwd.join("file.txt"), ours).unwrap();
    git(&cwd, &["commit", "-qam", "ours"]);
    if scenario == "existing" || scenario == "malformed" {
        let result = std::process::Command::new("git")
            .args(["merge", "--no-ff", "incoming"])
            .current_dir(&cwd)
            .output()
            .unwrap();
        assert!(!result.status.success());
        if scenario == "malformed" {
            std::fs::write(cwd.join("file.txt"), "<<<<<<< ours\nunterminated\n=======\n").unwrap();
        }
    }
    cwd
}
fn snapshot(cwd: &Path) -> Value {
    json!({"file":std::fs::read_to_string(cwd.join("file.txt")).ok(),"status":if cwd.join(".git").exists(){Some(git(cwd,&["status","--porcelain"]))}else{None}})
}
#[tokio::test]
async fn agent_end_complete_transcripts_and_repository_state_match() {
    if git_support::run_in_isolated_git_child("agent_end_complete_transcripts_and_repository_state_match") {
        return;
    }
    for scenario in [
        "not-repo",
        "dirty",
        "no-upstream",
        "clean",
        "fetch-failure",
        "conflict",
        "empty-conflict",
        "multi-conflict",
        "existing",
        "malformed",
    ] {
        let native = tempfile::tempdir().unwrap();
        let ts = tempfile::tempdir().unwrap();
        let cwd = setup(native.path(), scenario);
        let oracle_cwd = setup(ts.path(), scenario);
        let request = json!({"cwd":cwd,"steps":[{"op":"event","event":{"type":"agent_end","messages":[]}},{"op":"event","event":{"type":"agent_end","messages":[]}}]});
        if scenario == "not-repo" {
            harness::compare(
                "git-merge-and-resolve",
                harness::factory(example::git_merge_and_resolve_extension),
                request.clone(),
            )
            .await;
        }
        let actual = harness::run(harness::factory(example::git_merge_and_resolve_extension), &request).await;
        let mut oracle_request = request.clone();
        oracle_request["cwd"] = json!(oracle_cwd);
        let expected = harness::run_oracle("git-merge-and-resolve", &oracle_request);
        let normalize = |v: Value, root: &Path| {
            serde_json::from_str::<Value>(
                &serde_json::to_string(&v)
                    .unwrap()
                    .replace(root.to_str().unwrap(), "<tmp>"),
            )
            .unwrap()
        };
        assert_eq!(
            normalize(actual, native.path()),
            normalize(expected, ts.path()),
            "{scenario}"
        );
        assert_eq!(snapshot(&cwd), snapshot(&oracle_cwd), "{scenario}");
    }
}

// Shared support is compiled once per integration target.
#[path = "support/extensions.rs"]
pub mod shared_extensions;
#[path = "support/oracle.rs"]
pub mod shared_oracle;
