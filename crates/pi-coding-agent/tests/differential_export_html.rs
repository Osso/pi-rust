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

use pi_js_compat::js_json::JsString;
use pi_js_compat::js_json::encode_node_utf8;
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
    let input = dir.path().join("numeric-session.jsonl");
    let fixture = fs::read_to_string(fixture_path()).unwrap();
    let fixture = fixture.replace("\"arguments\":{", "\"arguments\":{\"2\":2.0,\"1\":1.0,");
    // Append arbitrary details to an existing message without changing session topology.
    let fixture = fixture
        .lines()
        .map(|line| {
            let mut entry: Value = serde_json::from_str(line).unwrap();
            if entry["message"]["role"] == "toolResult" && entry["message"].get("details").is_some() {
                entry["message"]["details"] = serde_json::from_str(r#"{"2":2.0,"1":1.0}"#).unwrap();
            }
            serde_json::to_string(&entry).unwrap()
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(fixture.contains("2.0"));
    fs::write(&input, fixture).unwrap();
    let input = input.display().to_string();

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
            &JsString::from(&input),
            Some(ExportOptions {
                output_path: Some(JsString::from(rust_output.display().to_string())),
                theme_name: theme_name.map(str::to_owned).map(JsString::from),
                ..ExportOptions::default()
            }),
        )
        .await
        .expect("rust export succeeds");

        let actual_html = fs::read_to_string(&&encode_node_utf8(&actual_path)).expect("read rust html");
        assert_same_html(&actual_html, expected_html, theme);
    }
}

#[test]
fn differential_custom_tools_and_overridden_assets() {
    let package = export_fixture::fixture_package_dir();
    for layout in ["src/core/export-html", "export-html"] {
        let path = package.path().join(layout).join("template.html");
        let template = fs::read_to_string(&path).unwrap();
        fs::write(
            path,
            template.replace("<title>", "<!--custom-package-template--><title>"),
        )
        .unwrap();
    }
    export_fixture::run_export_probe("differential_custom_tool_probe", package.path());
}

#[tokio::test]
#[ignore = "child process probe"]
async fn differential_custom_tool_probe() {
    if !env_probe::in_probe() {
        return;
    }
    use pi_ai::types::TextOrImageContent;
    use pi_coding_agent::core::export_html::{
        ExportOptions, RenderedToolResult, ToolHtmlRenderer, export_from_file, export_session_to_html,
    };
    use pi_coding_agent::core::session_manager::SessionManager;
    use pi_js_compat::js_json::JsValue;
    use std::{cell::RefCell, rc::Rc};
    struct Renderer {
        call_html: JsString,
        calls: RefCell<Vec<Value>>,
    }
    impl ToolHtmlRenderer for Renderer {
        fn render_call(&self, id: &str, name: &str, args: &JsValue) -> Option<JsString> {
            self.calls
                .borrow_mut()
                .push(json!({"id":id,"name":name,"args":args.to_serde().unwrap()}));
            Some(self.call_html.clone())
        }
        fn render_result(
            &self,
            _: &str,
            _: &str,
            _: &[TextOrImageContent],
            _: Option<&JsValue>,
            _: bool,
        ) -> Option<RenderedToolResult> {
            None
        }
    }
    let mut oracle = Oracle::new();
    let dir = tempfile::tempdir().unwrap();
    let session = SessionManager::open_read_only(&fixture_path().into(), None).unwrap();
    for (index, call_html) in ["", "<b>custom call</b>"].into_iter().enumerate() {
        let expected = oracle.request("exportLive", json!({"inputPath":fixture_path(),"outputPath":dir.path().join(format!("ts{index}.html")),"callHtml":call_html}));
        let renderer = Rc::new(Renderer {
            call_html: call_html.into(),
            calls: RefCell::new(Vec::new()),
        });
        let actual = export_session_to_html(
            &session,
            None,
            Some(ExportOptions {
                output_path: Some(dir.path().join(format!("native{index}.html")).to_str().unwrap().into()),
                tool_renderer: Some(renderer.clone()),
                ..Default::default()
            }),
        )
        .await
        .unwrap();
        assert_eq!(json!(*renderer.calls.borrow()), expected["calls"]);
        let html = fs::read_to_string(encode_node_utf8(&actual)).unwrap();
        assert_same_html(&html, expected["html"].as_str().unwrap(), call_html);
        assert!(html.contains("custom-package-template"));
    }
    let package = PathBuf::from(std::env::var_os("PI_PACKAGE_DIR").unwrap());
    fs::remove_file(package.join("src/core/export-html/template.css")).unwrap();
    fs::remove_file(package.join("export-html/template.css")).unwrap();
    let expected = oracle.request(
        "exportFromFile",
        json!({"inputPath":fixture_path(),"outputPath":dir.path().join("ts-missing.html")}),
    );
    assert_eq!(expected["ok"], false);
    let error = export_from_file(
        &fixture_path().into(),
        Some(ExportOptions::from_output_path(
            dir.path().join("native-missing.html").to_str().unwrap(),
        )),
    )
    .await
    .unwrap_err();
    assert_eq!(
        encode_node_utf8(&error.message()),
        expected["error"]
            .as_str()
            .unwrap()
            .replace("/src/core/export-html/", "/export-html/")
    );
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
    let error = export_from_file(
        &JsString::from(missing),
        Some(ExportOptions::from_output_path("/tmp/out.html")),
    )
    .await
    .expect_err("missing file must fail");
    assert_eq!(error.message(), expected["error"].as_str().unwrap());
}
