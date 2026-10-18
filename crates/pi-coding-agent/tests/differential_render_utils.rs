#[path = "support/oracle.rs"]
mod oracle;

use pi_agent_core::agent::AgentToolResult;
use pi_ai::types::{TextContent, TextOrImageContent};
use pi_coding_agent::core::tools::render_utils::{
    get_text_output, normalize_display_text, replace_tabs, shorten_path, str_value,
};
use serde_json::{Value, json};

#[test]
fn coding_agent_render_utils_matches_the_pinned_typescript_source() {
    let home = std::env::var("HOME").expect("HOME");
    let mut cases = Vec::new();
    for path in [
        format!("{home}/projects/file.txt"),
        home.clone(),
        "/tmp/file.txt".to_owned(),
        "relative/path".to_owned(),
        String::new(),
    ] {
        cases.push((json!({"kind": "shorten", "path": path}), path_result(&path)));
    }
    for value in [
        json!("text"),
        json!(""),
        json!(null),
        json!(4),
        json!(true),
        json!({"a": 1}),
        json!([1, 2]),
    ] {
        let expected = str_value(Some(&value));
        cases.push((
            json!({"kind": "str", "value": value}),
            json!({"ok": true, "value": expected}),
        ));
    }
    for text in ["a\tb", "\t\t", "no tabs", "a\tb\tc"] {
        cases.push((
            json!({"kind": "tabs", "text": text}),
            json!({"ok": true, "value": replace_tabs(text)}),
        ));
    }
    for text in ["a\r\nb", "a\rb\rc", "plain"] {
        cases.push((
            json!({"kind": "display", "text": text}),
            json!({"ok": true, "value": normalize_display_text(text)}),
        ));
    }
    for texts in [
        vec!["one"],
        vec!["one", "two"],
        vec!["\u{1b}[31mred\u{1b}[0m\r\n", "x\u{0}y"],
    ] {
        let content = texts
            .iter()
            .map(|text| json!({"type": "text", "text": text}))
            .collect::<Vec<_>>();
        let result = AgentToolResult {
            content: texts
                .iter()
                .map(|text| TextOrImageContent::Text(TextContent::new(*text)))
                .collect(),
            details: None,
            usage: None,
            added_tool_names: None,
            is_error: None,
            terminate: None,
        };
        cases.push((
            json!({"kind": "textOutput", "content": content, "showImages": true}),
            json!({"ok": true, "value": get_text_output(Some(&result), true)}),
        ));
    }

    let mut oracle = oracle::TsOracle::spawn(
        "coding-agent-render-utils-oracle.ts",
        "packages/coding-agent/src/core/tools/render-utils.ts",
    );
    let expected = oracle.request(&json!({
        "id": 27_100,
        "operation": "coding_agent_render_utils",
        "cases": cases.iter().map(|(request, _)| request).collect::<Vec<_>>(),
    }));
    assert_eq!(expected["ok"], Value::Bool(true), "{expected}");
    let expected_results = expected["output"].as_array().expect("oracle output array");
    assert_eq!(expected_results.len(), cases.len());
    for (index, ((_, actual), expected)) in cases.iter().zip(expected_results).enumerate() {
        assert_eq!(actual, expected, "case {index}");
    }
}

fn path_result(path: &str) -> Value {
    json!({"ok": true, "value": shorten_path(path)})
}
