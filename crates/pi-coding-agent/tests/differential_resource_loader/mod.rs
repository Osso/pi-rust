#[path = "../support/oracle.rs"]
mod oracle;

use std::fs;
use std::path::Path;

use pi_coding_agent::core::resource_loader::{load_rules_files_from_dir, load_rules_from_dir};
use serde_json::{Value, json};
use tempfile::TempDir;

pub(crate) fn run() {
    let mut oracle = oracle::TsOracle::spawn(
        "coding-agent-resource-loader-oracle.ts",
        "packages/coding-agent/src/core/resource-loader.ts",
    );

    let temp = TempDir::new().expect("temp dir");
    write_rules_fixture(temp.path());
    let missing = temp.path().join("missing").to_string_lossy().into_owned();

    for dir in [temp.path().to_string_lossy().into_owned(), missing] {
        let response = oracle.request(&json!({"id": 1, "operation": "rules", "dir": &dir}));
        assert_eq!(response["ok"], true, "oracle failed: {response}");

        let files = load_rules_files_from_dir(&dir).expect("rules files load");
        let actual_files: Vec<Value> = files
            .iter()
            .map(|file| json!({"path": file.path, "content": file.content}))
            .collect();
        assert_eq!(Value::Array(actual_files), response["output"]["files"]);

        let content = load_rules_from_dir(&dir).expect("rules content load");
        assert_eq!(
            content.map_or(Value::Null, Value::String),
            response["output"]["content"]
        );
    }
}

fn write_rules_fixture(root: &Path) {
    fs::write(root.join("b-rule.md"), "Second rule.\n\n").unwrap();
    fs::write(root.join("a-rule.md"), "  First rule.  \n").unwrap();
    fs::write(root.join("empty.md"), "   \n\n").unwrap();
    fs::write(root.join("notes.txt"), "ignored").unwrap();
    fs::write(root.join("éclair.md"), "Accented rule.").unwrap();
    fs::write(root.join("Z-rule.md"), "Upper rule.").unwrap();
}
