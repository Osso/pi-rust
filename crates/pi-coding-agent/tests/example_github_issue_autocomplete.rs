#[path = "../examples/extensions/github_issue_autocomplete.rs"]
mod example;
use crate::shared_extensions as extension_support;
#[path = "support/extension_examples_b.rs"]
mod harness;
use crate::shared_oracle as oracle;
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
            harness::compare("github-issue-autocomplete",Arc::new(|api| {let result=example::github_issue_autocomplete_extension(api);Box::pin(async move {result})}),json!({"cwd":cwd,"path":path,"cases":[{"event":"session_start"},{"event":"autocomplete","text":"#"},{"event":"autocomplete","text":"#1"},{"event":"autocomplete","text":"#café"},{"event":"autocomplete","text":"#9999"},{"event":"autocomplete","text":"not#1"},{"event":"autocomplete","text":"\t#2"},{"event":"autocomplete","text":"#Fix"},{"event":"autocomplete","text":"#","aborted":true},{"event":"autocomplete","text":"#x#"},{"event":"autocomplete","text":"\n#1"},{"event":"autocomplete","text":"#�"},{"event":"session_start"},{"event":"autocomplete","text":"#101"}]})).await;
        }
    }
    assert!(
        std::process::Command::new("git")
            .args([
                "-C",
                cwd,
                "remote",
                "set-url",
                "origin",
                "https://github.com/owner/repo.git"
            ])
            .status()
            .unwrap()
            .success()
    );
    std::fs::write(
        temp.path().join("response"),
        json!({"code":0,"stdout":"[]","stderr":""}).to_string(),
    )
    .unwrap();
    let actual = harness::compare("github-issue-autocomplete", Arc::new(|api| {
        let result = example::github_issue_autocomplete_extension(api);
        Box::pin(async move { result })
    }), json!({"cwd":cwd,"path":path,"cases":[
        {"event":"session_start"},
        {"event":"autocomplete","text":"files: ","lines":["heading","files: abc","tail"],"line":1,"col":7,"force":true},
        {"event":"autocomplete","text":"files: ","lines":["heading","files: abc","tail"],"line":1,"col":7,"force":true,"aborted":true},
        {"event":"autocomplete","text":"files: ","lines":["heading","files: abc","tail"],"line":1,"col":8,"prefix":"files: a"},
        {"event":"autocomplete","text":"tail","lines":["heading","files: abc","tail"],"line":2,"col":4}
    ]})).await;
    assert_eq!(actual["cases"][1]["result"]["trigger"], true);
    assert_eq!(actual["cases"][2]["result"]["trigger"], true);
    assert_eq!(actual["cases"][3]["result"]["trigger"], false);
    assert_eq!(actual["cases"][4]["result"]["trigger"], false);
    let fallback: serde_json::Value =
        serde_json::from_str(actual["cases"][2]["result"]["suggestions"]["prefix"].as_str().unwrap()).unwrap();
    assert_eq!(
        fallback,
        json!({"lines":["heading","files: abc","tail"],"line":1,"col":7,"force":true,"aborted":true})
    );
    // Non-git cwd has the distinct resolution error and installs no wrapper.
    let nongit = tempfile::tempdir().unwrap();
    harness::compare("github-issue-autocomplete",Arc::new(|api| {let result=example::github_issue_autocomplete_extension(api);Box::pin(async move {result})}),json!({"cwd":nongit.path(),"path":path,"cases":[{"event":"session_start"},{"event":"autocomplete","text":"#1"}]})).await;
    pi_js_compat::process_env::remove("PATH");
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
