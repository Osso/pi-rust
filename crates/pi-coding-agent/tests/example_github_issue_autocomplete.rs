#[path = "../examples/extensions/github_issue_autocomplete.rs"]
mod example;
#[path = "support/extension_examples_b.rs"]
mod harness;
use serde_json::json;
use std::{os::unix::fs::PermissionsExt, sync::Arc};

#[tokio::test]
async fn fixture_requests_cache_errors_and_completions_match() {
    let temp = tempfile::tempdir().unwrap();
    let cwd = temp.path().to_str().unwrap();
    let bin = temp.path().join("bin");
    std::fs::create_dir(&bin).unwrap();
    let gh = bin.join("gh");
    std::fs::write(&gh, "#!/usr/bin/python3\nimport json,sys,pathlib\np=pathlib.Path.cwd()\nwith (p/'requests').open('a') as f: f.write(json.dumps(sys.argv[1:])+'\\n')\nx=json.loads((p/'response').read_text())\nprint(x['stdout'],end='')\nprint(x['stderr'],end='',file=sys.stderr)\nsys.exit(x['code'])\n").unwrap();
    std::fs::set_permissions(&gh, std::fs::Permissions::from_mode(0o755)).unwrap();
    let path = format!("{}:{}", bin.display(), std::env::var("PATH").unwrap());
    pi_js_compat::process_env::set("PATH", &path);
    assert!(
        std::process::Command::new("git")
            .args(["init", "-q", cwd])
            .status()
            .unwrap()
            .success()
    );
    let mut issues = (1..=25)
        .map(|i| json!({"number":i,"title":format!("Fix issue {i}"),"state":"OPEN"}))
        .collect::<Vec<_>>();
    issues.push(json!({"number":101,"title":"🦀 Unicode café","state":"CLOSED"}));
    for remote in [
        "git@github.com:owner/repo.git",
        "https://github.com/owner/repo.git",
        "http://github.com/owner/repo",
        "https://example.org/owner/repo",
    ] {
        let _ = std::process::Command::new("git")
            .args(["-C", cwd, "remote", "remove", "origin"])
            .output()
            .unwrap();
        assert!(
            std::process::Command::new("git")
                .args(["-C", cwd, "remote", "add", "origin", remote])
                .status()
                .unwrap()
                .success()
        );
        for response in [
            json!({"code":0,"stdout":serde_json::to_string(&issues).unwrap(),"stderr":""}),
            json!({"code":0,"stdout":"[]","stderr":""}),
            json!({"code":0,"stdout":"invalid","stderr":""}),
            json!({"code":7,"stdout":"","stderr":"  fixture failure\n"}),
            json!({"code":9,"stdout":"","stderr":""}),
        ] {
            std::fs::write(temp.path().join("response"), response.to_string()).unwrap();
            harness::compare("github-issue-autocomplete",Arc::new(|api| {let result=example::github_issue_autocomplete_extension(api);Box::pin(async move {result})}),json!({"cwd":cwd,"path":path,"cases":[{"event":"session_start"},{"event":"autocomplete","text":"#"},{"event":"autocomplete","text":"#1"},{"event":"autocomplete","text":"#café"},{"event":"autocomplete","text":"#9999"},{"event":"autocomplete","text":"not#1"},{"event":"autocomplete","text":"\t#2"},{"event":"autocomplete","text":"#Fix"},{"event":"autocomplete","text":"#","aborted":true},{"event":"autocomplete","text":"#x#"},{"event":"autocomplete","text":"\n#1"},{"event":"session_start"},{"event":"autocomplete","text":"#101"}]})).await;
        }
    }
    // Non-git cwd has the distinct resolution error and installs no wrapper.
    let nested = temp.path().join("not-a-repo");
    std::fs::create_dir(&nested).unwrap();
    // git still finds its ancestor, so use a separate root.
    let nongit = tempfile::tempdir().unwrap();
    harness::compare("github-issue-autocomplete",Arc::new(|api| {let result=example::github_issue_autocomplete_extension(api);Box::pin(async move {result})}),json!({"cwd":nongit.path(),"path":path,"cases":[{"event":"session_start"},{"event":"autocomplete","text":"#1"}]})).await;
    pi_js_compat::process_env::remove("PATH");
}
