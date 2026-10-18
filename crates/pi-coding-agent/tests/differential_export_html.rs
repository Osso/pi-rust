//! Differential tests: the pinned TypeScript HTML export versus the Rust port.
//!
//! Export resolves built-in themes from `PI_PACKAGE_DIR`, so the comparison runs
//! in a child process with a fixture package directory.

#[path = "support/env_probe.rs"]
mod env_probe;
#[path = "support/export_fixture.rs"]
mod export_fixture;
#[path = "support/oracle.rs"]
mod oracle;

use std::fs;
use std::path::PathBuf;

use serde_json::{Value, json};

struct Oracle {
    inner: oracle::TsOracle,
    next_id: u64,
}

impl Oracle {
    fn new() -> Self {
        Self {
            inner: oracle::TsOracle::spawn("coding-agent-export-html-oracle.ts", "packages/coding-agent/src"),
            next_id: 0,
        }
    }

    fn request(&mut self, operation: &str, body: Value) -> Value {
        self.next_id += 1;
        let mut request = body;
        request["id"] = json!(self.next_id);
        request["module"] = json!("core/export-html/index.ts");
        request["operation"] = json!(operation);
        let response = self.inner.request(&request);
        assert_eq!(response["ok"], json!(true), "oracle failed: {response}");
        response["output"].clone()
    }
}

fn assert_same_html(actual: &str, expected: &str, theme: &str) {
    if actual == expected {
        return;
    }
    let index = actual
        .bytes()
        .zip(expected.bytes())
        .position(|(left, right)| left != right)
        .unwrap_or_else(|| actual.len().min(expected.len()));
    let start = index.saturating_sub(120);
    let end = (index + 120).min(actual.len().max(expected.len()));
    panic!(
        "HTML mismatch for theme {theme:?} at byte {index}\nactual: {:?}\nexpected: {:?}\nlengths: {} vs {}",
        actual.get(start..end.min(actual.len())).unwrap_or_default(),
        expected.get(start..end.min(expected.len())).unwrap_or_default(),
        actual.len(),
        expected.len()
    );
}

fn fixture_path() -> String {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/export-session.jsonl")
        .display()
        .to_string()
}

#[test]
fn differential_export_from_file_matches_byte_for_byte() {
    let package = export_fixture::fixture_package_dir();
    export_fixture::run_export_probe("differential_export_probe", package.path());
}

#[tokio::test]
#[ignore = "child process probe"]
async fn differential_export_probe() {
    if !env_probe::in_probe() {
        return;
    }
    use pi_coding_agent::core::export_html::{ExportOptions, export_from_file};

    let mut oracle = Oracle::new();
    let dir = tempfile::tempdir().expect("temp dir");
    let input = fixture_path();

    for theme in ["dark", "light", ""] {
        let theme_name = (!theme.is_empty()).then_some(theme);
        let ts_output = dir.path().join(format!("ts-{theme}.html"));
        let rust_output = dir.path().join(format!("rust-{theme}.html"));

        let expected = oracle.request(
            "exportFromFile",
            json!({
                "inputPath": input,
                "outputPath": ts_output.display().to_string(),
                "themeName": theme_name,
            }),
        );
        assert_eq!(expected["ok"], json!(true), "oracle export failed: {expected}");
        let expected_html = expected["html"].as_str().expect("html string");

        let actual_path = export_from_file(
            &input,
            Some(ExportOptions {
                output_path: Some(rust_output.display().to_string()),
                theme_name: theme_name.map(str::to_owned),
                ..ExportOptions::default()
            }),
        )
        .await
        .expect("rust export succeeds");

        let actual_html = fs::read_to_string(&actual_path).expect("read rust html");
        assert_same_html(&actual_html, expected_html, theme);
    }
}

#[test]
fn differential_missing_file_error_message() {
    let package = export_fixture::fixture_package_dir();
    export_fixture::run_export_probe("differential_missing_file_probe", package.path());
}

#[tokio::test]
#[ignore = "child process probe"]
async fn differential_missing_file_probe() {
    if !env_probe::in_probe() {
        return;
    }
    use pi_coding_agent::core::export_html::{ExportOptions, export_from_file};

    let mut oracle = Oracle::new();
    let missing = "/tmp/pi-export-missing-session.jsonl";
    let expected = oracle.request(
        "exportFromFile",
        json!({ "inputPath": missing, "outputPath": "/tmp/out.html" }),
    );
    assert_eq!(expected["ok"], json!(false));
    let error = export_from_file(missing, Some(ExportOptions::from_output_path("/tmp/out.html")))
        .await
        .expect_err("missing file must fail");
    assert_eq!(error.message(), expected["error"].as_str().unwrap());
}
