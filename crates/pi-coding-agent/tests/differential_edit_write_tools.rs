use crate::shared_oracle as oracle;

use std::sync::Arc;

use pi_agent_core::agent::AgentTool;
use pi_coding_agent::core::tools::edit::EditTool;
use pi_coding_agent::core::tools::write::WriteTool;
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::{JsString, JsValue as Value};

struct Case {
    kind: &'static str,
    params: Value,
    original: Option<&'static str>,
    file: std::path::PathBuf,
}

#[test]
fn coding_agent_edit_prepare_arguments_match_the_pinned_typescript_source() {
    let cwd = "/tmp/pi-edit-prepare-differential";
    let cases = [
        json!(null),
        json!("garbage"),
        json!({"path": "file.txt", "oldText": "before", "newText": "after"}),
        json!({
            "path": "file.txt",
            "edits": [{"oldText": "a", "newText": "b"}],
            "oldText": "c",
            "newText": "d"
        }),
        json!({"path": "file.txt", "edits": "[{\"oldText\":\"a\",\"newText\":\"b\"}]"}),
        json!({"path": "file.txt", "edits": "not json"}),
        json!({"path": "file.txt", "oldText": 1, "newText": "b"}),
        json!({"leading":"lead","edits":[{"oldText":"a","newText":"b"}],"middle":true,"oldText":"c","trailing":42,"newText":"d"}),
        json!({"leading":"lead","edits":"[{\"oldText\":\"a\",\"newText\":\"b\"}]","middle":true,"oldText":"c","trailing":42,"newText":"d"}),
        json!({"edits":"null","oldText":"","newText":"replacement","tail":true}),
        json!({"edits":"{}","oldText":"before","newText":"after"}),
        json!({"edits":"1","oldText":"before","newText":"after"}),
        json!({"edits":false,"oldText":JsString::from_utf16(vec![0xd800]),"newText":JsString::from_utf16(vec![0xdfff]),"tail":"raw"}),
        json!({"__proto__":{"oldText":"inherited"},"edits":[],"oldText":"own","newText":"replacement","tail":null}),
    ];

    let mut oracle = oracle::TsOracle::spawn(
        "coding-agent-edit-write-tool-oracle.ts",
        "packages/coding-agent/src/core/tools/edit.ts",
    );
    let expected = oracle.request(&json!({
        "id": 27_301,
        "operation": "coding_agent_edit_prepare_arguments",
        "cases": cases
            .iter()
            .map(|params| json!({"kind": "prepare", "cwd": cwd, "params": params}))
            .collect::<Vec<_>>(),
    }));
    assert_eq!(expected["ok"], Value::Bool(true), "{expected:?}");
    let expected_results = expected["output"].as_array().expect("oracle output array");
    assert_eq!(expected_results.len(), cases.len());

    let definition = pi_coding_agent::core::tools::edit::create_edit_tool_definition(
        JsString::from(cwd),
        pi_coding_agent::core::tools::edit::EditToolOptions::default(),
    );
    let prepare = definition.prepare_arguments.expect("prepare arguments forwarded");
    for (index, (case, expected)) in cases.iter().zip(expected_results).enumerate() {
        let actual = prepare(case).expect("prepare arguments");
        assert_eq!(
            json!({"ok": true, "value": actual, "serialized":pi_js_compat::js_json::stringify_lossless(&actual)}),
            *expected,
            "case {index}"
        );
    }
}

#[test]
fn coding_agent_edit_and_write_tools_match_the_pinned_typescript_source() {
    let temp = tempfile::tempdir().expect("temp dir");
    let cwd = temp.path().to_str().expect("utf-8 test path").to_owned();
    let edit_file = temp.path().join("edit.txt");
    let multi_file = temp.path().join("multi.txt");
    let ambiguous_file = temp.path().join("ambiguous.txt");
    let missing_file = temp.path().join("missing.txt");
    let write_file = temp.path().join("nested/write.txt");

    let cases = vec![
        Case {
            kind: "edit",
            params: json!({"path": edit_file.to_str().expect("utf-8 test path"), "edits": [{"oldText": "world", "newText": "universe"}]}),
            original: Some("Hello, world!"),
            file: edit_file.clone(),
        },
        Case {
            kind: "edit",
            params: json!({"path": multi_file.to_str().expect("utf-8 test path"), "edits": [
                {"oldText": "alpha", "newText": "one"},
                {"oldText": "gamma", "newText": "three"}
            ]}),
            original: Some("alpha\nbeta\ngamma\ndelta"),
            file: multi_file.clone(),
        },
        Case {
            kind: "edit",
            params: json!({"path": ambiguous_file.to_str().expect("utf-8 test path"), "edits": [{"oldText": "same", "newText": "x"}]}),
            original: Some("same\nsame"),
            file: ambiguous_file.clone(),
        },
        Case {
            kind: "edit",
            params: json!({"path": missing_file.to_str().expect("utf-8 test path"), "edits": [{"oldText": "a", "newText": "b"}]}),
            original: None,
            file: missing_file.clone(),
        },
        Case {
            kind: "write",
            params: json!({"path": write_file.to_str().expect("utf-8 test path"), "content": "written 😀 content"}),
            original: None,
            file: write_file.clone(),
        },
    ];

    for case in &cases {
        if let Some(original) = case.original {
            std::fs::write(&case.file, original).expect("fixture");
        } else {
            let _ = std::fs::remove_file(&case.file);
        }
    }

    let mut oracle = oracle::TsOracle::spawn(
        "coding-agent-edit-write-tool-oracle.ts",
        "packages/coding-agent/src/core/tools/edit.ts",
    );
    let expected = oracle.request(&json!({
        "id": 27_300,
        "operation": "coding_agent_edit_write_tools",
        "cases": cases
            .iter()
            .map(|case| json!({"kind": case.kind, "cwd": cwd, "params": case.params}))
            .collect::<Vec<_>>(),
    }));
    assert_eq!(expected["ok"], Value::Bool(true), "{expected:?}");
    let expected_results = expected["output"].as_array().expect("oracle output array");
    assert_eq!(expected_results.len(), cases.len());
    let expected_files = cases
        .iter()
        .map(|case| std::fs::read_to_string(&case.file).ok())
        .collect::<Vec<_>>();

    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    for (index, (case, expected)) in cases.iter().zip(expected_results).enumerate() {
        if let Some(original) = case.original {
            std::fs::write(&case.file, original).expect("fixture");
        } else {
            let _ = std::fs::remove_file(&case.file);
        }
        let tool: Arc<dyn AgentTool> = if case.kind == "edit" {
            Arc::new(EditTool::new(&cwd))
        } else {
            Arc::new(WriteTool::new(&cwd))
        };
        let actual = match runtime.block_on(tool.execute(
            format!("{}-differential", case.kind),
            case.params.clone(),
            None,
            Arc::new(|_result| Ok(())),
            None,
        )) {
            Ok(result) => json!({"ok": true, "value": result}),
            Err(error) => json!({"ok": false, "error": error.message}),
        };
        assert_eq!(&actual, expected, "case {index}");
        assert_eq!(
            std::fs::read_to_string(&case.file).ok(),
            expected_files[index],
            "file after case {index}"
        );
    }
}
