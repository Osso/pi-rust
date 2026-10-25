#![cfg(unix)]

use crate::shared_oracle as oracle;

use pi_agent_core::agent::AgentTool;
use pi_ai::request::BoxFuture;
use pi_coding_agent::core::tools::ls::{LocalLsOperations, LsOperations, LsTool, LsToolOptions};
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::{self, JsString, JsValue};
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::{Path, PathBuf};
use std::sync::Arc;

const CASES: [&str; 8] = [
    "empty",
    "entries",
    "missing",
    "file",
    "unreadable",
    "remove-after-exists",
    "remove-after-stat",
    "file-after-stat",
];

fn create_fixture(cwd: &Path, scenario: &str) -> PathBuf {
    let target = cwd.join("target");
    std::fs::create_dir(cwd).unwrap();
    if scenario == "missing" {
        return target;
    }
    if scenario == "file" {
        std::fs::write(&target, "file").unwrap();
        return target;
    }
    std::fs::create_dir(&target).unwrap();
    if scenario == "entries" {
        std::fs::write(target.join(".hidden"), "").unwrap();
        std::fs::write(target.join("Alpha"), "").unwrap();
        std::fs::create_dir(target.join("dir")).unwrap();
        symlink("dir", target.join("link")).unwrap();
        symlink("missing", target.join("broken")).unwrap();
        symlink("cycle", target.join("cycle")).unwrap();
        std::fs::write(target.join(std::ffi::OsStr::from_bytes(&[255])), "").unwrap();
    }
    if scenario == "unreadable" {
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o000)).unwrap();
    }
    target
}

struct FixtureOperations {
    scenario: &'static str,
    target: PathBuf,
}

impl LsOperations for FixtureOperations {
    fn exists<'a>(&'a self, path: &'a JsString) -> BoxFuture<'a, io::Result<bool>> {
        Box::pin(async move {
            let exists = LocalLsOperations.exists(path).await?;
            if exists && self.scenario == "remove-after-exists" {
                std::fs::remove_dir(&self.target)?;
            }
            Ok(exists)
        })
    }

    fn is_directory<'a>(&'a self, path: &'a JsString) -> BoxFuture<'a, io::Result<bool>> {
        Box::pin(async move {
            let directory = LocalLsOperations.is_directory(path).await?;
            if path == &JsString::from(self.target.to_str().unwrap()) {
                match self.scenario {
                    "remove-after-stat" => std::fs::remove_dir(&self.target)?,
                    "file-after-stat" => {
                        std::fs::remove_dir(&self.target)?;
                        std::fs::write(&self.target, "replacement")?;
                    }
                    _ => {}
                }
            }
            Ok(directory)
        })
    }

    fn readdir<'a>(&'a self, path: &'a JsString) -> BoxFuture<'a, io::Result<Vec<JsString>>> {
        LocalLsOperations.readdir(path)
    }
}

async fn execute_fixture(cwd: &Path, scenario: &'static str) -> JsValue {
    let target = create_fixture(cwd, scenario);
    let operations = Arc::new(FixtureOperations {
        scenario,
        target: target.clone(),
    });
    let tool = LsTool::with_options(
        cwd.to_str().unwrap(),
        LsToolOptions {
            operations: Some(operations),
        },
    );
    let result = tool
        .execute(
            "filesystem".into(),
            json!({"path":"target"}),
            None,
            Arc::new(|_| Ok(())),
            None,
        )
        .await;
    if scenario == "unreadable" {
        std::fs::set_permissions(target, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    std::fs::remove_dir_all(cwd).unwrap();
    match result {
        Ok(value) => json!({"ok":true,"value":js_json::to_value(&value).unwrap()}),
        Err(error) => json!({"ok":false,"error":error.message}),
    }
}

#[tokio::test]
async fn local_ls_matches_frozen_filesystem_results_and_failure_boundaries() {
    let root = tempfile::tempdir().unwrap();
    let mut source = oracle::TsOracle::spawn("ls-filesystem-oracle.ts", "packages/coding-agent/src/core/tools/ls.ts");
    let expected: JsValue = source.request(&json!({"id":1,"cwd":root.path(),"cases":CASES}));
    assert_eq!(expected["ok"], JsValue::Bool(true), "{expected:?}");
    let expected = expected["output"].as_array().unwrap();
    assert_eq!(expected.len(), CASES.len());
    assert!(expected[4]["error"].as_string().unwrap().contains("EACCES"));
    let mut failures = Vec::new();
    for (scenario, expected) in CASES.into_iter().zip(expected) {
        let actual = execute_fixture(&root.path().join(scenario), scenario).await;
        if &actual != expected {
            failures.push(format!(
                "{scenario}: {} != {}",
                js_json::stringify_lossless(&actual),
                js_json::stringify_lossless(expected)
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
