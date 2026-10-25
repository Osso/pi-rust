use crate::shared_oracle as oracle;
use pi_coding_agent::utils::paths::{PathInputOptions, normalize_path};
use serde_json::{Value, json};
use std::process::Command;

#[test]
fn path_consumers_use_passwd_when_home_is_unset() {
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--ignored", "--exact", "path_home_child", "--nocapture"])
        .env_remove("HOME")
        .env("USERPROFILE", "/wrong-posix-home")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
#[test]
#[ignore = "isolated HOME environment"]
fn path_home_child() {
    let mut source = oracle::TsOracle::spawn(
        "coding-agent-paths-oracle.ts",
        "packages/coding-agent/src/utils/paths.ts",
    );
    let expected:Value=source.request(&json!({"cases":[{"kind":"normalize","name":"home","input":"~"},{"kind":"normalize","name":"child","input":"~/child"}]}));
    let actual:Vec<_>=[("home","~"),("child","~/child")].into_iter().map(|(name,input)|json!({"name":name,"ok":true,"value":normalize_path(input,&PathInputOptions::default()).unwrap()})).collect();
    assert_eq!(json!(actual), expected["output"]["results"]);
}

// Shared support is compiled once per integration target.
#[path = "support/oracle.rs"]
pub mod shared_oracle;
