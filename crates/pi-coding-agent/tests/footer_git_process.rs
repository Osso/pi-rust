use crate::shared_oracle as oracle;
use pi_coding_agent::core::footer_data_provider::{FooterDataProvider, GitBranchRunner, SystemGitBranchRunner};
use pi_js_compat::{
    js_json,
    js_json::{JsString, JsValue, parse_lossless, stringify_lossless},
};
use std::os::unix::fs::PermissionsExt;

#[tokio::test]
async fn git_process_exit_decoding_and_default_output_limits_match_frozen() {
    const PROBE: &str = "PI_FOOTER_GIT_PROBE";
    if let Ok(path) = std::env::var(PROBE) {
        let cwd = std::env::var("PI_FOOTER_GIT_CWD").unwrap();
        let provider = FooterDataProvider::new(cwd.as_str());
        let sync = provider.get_git_branch();
        let branch = SystemGitBranchRunner
            .resolve_async(&JsString::from(cwd))
            .await
            .unwrap_or_else(|| "detached".into());
        provider.dispose();
        std::fs::write(path, stringify_lossless(&js_json!({"sync":sync,"async":branch}))).unwrap();
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let cwd = root.path().join("repo");
    let bin = root.path().join("bin");
    std::fs::create_dir_all(cwd.join(".git")).unwrap();
    std::fs::create_dir(&bin).unwrap();
    std::fs::write(cwd.join(".git/HEAD"), "ref: refs/heads/.invalid\n").unwrap();
    let node = std::process::Command::new("node")
        .args(["-p", "process.execPath"])
        .output()
        .unwrap();
    assert!(node.status.success());
    std::os::unix::fs::symlink(String::from_utf8(node.stdout).unwrap().trim(), bin.join("node")).unwrap();
    let script = r#"#!/usr/bin/python3
import os,sys
case=os.environ['PI_FOOTER_GIT_CASE']
if sys.argv[1:] != ['--no-optional-locks','symbolic-ref','--quiet','--short','HEAD']: sys.exit(9)
if case=='small': os.write(1,b' \xe2\x80\x83branch\xc2\x85 \n')
elif case=='invalid': os.write(1,b' b\xed\xa0\x80\n')
elif case=='stdout_limit': os.write(1,b'x'*1048577)
elif case=='stderr_limit': os.write(2,b'x'*1048577);os.write(1,b'valid\n')
elif case=='empty': pass
elif case=='failure': os.write(1,b'ignored');sys.exit(7)
"#;
    let git = bin.join("git");
    let mut actual = Vec::new();
    let mut expected = Vec::new();
    for case in [
        "small",
        "invalid",
        "stdout_limit",
        "stderr_limit",
        "empty",
        "failure",
        "missing",
    ] {
        if case == "missing" {
            std::fs::remove_file(&git).unwrap();
        } else {
            std::fs::write(&git, script).unwrap();
            std::fs::set_permissions(&git, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let mut source = oracle::TsOracle::spawn_with_environment(
            "footer-provider-oracle.mts",
            "packages/coding-agent/src/core/footer-data-provider.ts",
            &[
                ("PATH", bin.clone().into_os_string()),
                ("PI_FOOTER_GIT_CASE", case.into()),
            ],
        );
        let value = source.request(&js_json!({"operation":"git","cwd":cwd.to_str().unwrap()}));
        expected.push(js_json!({"case":case,"result":value}));
        let output_path = root.path().join("native.json");
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "git_process_exit_decoding_and_default_output_limits_match_frozen",
                "--nocapture",
            ])
            .env(PROBE, &output_path)
            .env("PI_FOOTER_GIT_CWD", &cwd)
            .env("PI_FOOTER_GIT_CASE", case)
            .env("PATH", &bin)
            .output()
            .unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        actual.push(
            js_json!({"case":case,"result":parse_lossless(std::fs::read_to_string(output_path).unwrap()).unwrap()}),
        );
    }
    assert_eq!(JsValue::Array(actual), JsValue::Array(expected));
}

// Shared support is compiled once per integration target.
#[path = "support/oracle.rs"]
pub mod shared_oracle;
