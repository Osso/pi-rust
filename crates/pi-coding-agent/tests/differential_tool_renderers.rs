#[path = "support/oracle.rs"]
mod oracle;

use pi_agent_core::agent::AgentToolResult;
use pi_ai::types::{TextContent, TextOrImageContent};
use pi_coding_agent::core::extensions::types::ToolRenderContext;
use pi_coding_agent::core::tools::bash::render_call as bash_render_call;
use pi_coding_agent::core::tools::code_index::{
    code_index_render_result, outline_render_call, references_render_call, symbol_render_call,
};
use pi_coding_agent::core::tools::edit::{render_call as edit_render_call, render_result as edit_render_result};
use pi_coding_agent::core::tools::find::{render_call as find_render_call, render_result as find_render_result};
use pi_coding_agent::core::tools::grep::{render_call as grep_render_call, render_result as grep_render_result};
use pi_coding_agent::core::tools::ls::{render_call as ls_render_call, render_result as ls_render_result};
use pi_coding_agent::core::tools::read::{render_call as read_render_call, render_result as read_render_result};
use pi_coding_agent::core::tools::write::{render_call as write_render_call, render_result as write_render_result};
use pi_js_compat::js_json::JsString;
use pi_js_compat::js_json::JsValue;
use serde_json::{Value, json};

fn dark_theme() -> std::sync::Arc<pi_coding_agent::modes::interactive::theme::Theme> {
    use pi_coding_agent::modes::interactive::theme::{ColorMode, load_theme_from_path, set_theme_instance};
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("theme/dark.json");
    let theme = std::sync::Arc::new(
        load_theme_from_path(
            &JsString::from(path.to_str().expect("fixture path must be UTF-8")),
            Some(ColorMode::Truecolor),
        )
        .expect("built-in dark theme"),
    );
    set_theme_instance(std::sync::Arc::clone(&theme));
    theme
}

fn context(args: Value, expanded: bool, is_error: bool, show_images: bool) -> ToolRenderContext {
    ToolRenderContext {
        args: JsValue::from(args),
        tool_call_id: "render-differential".to_owned(),
        invalidate: std::sync::Arc::new(|| {}),
        last_component: None,
        call_component: None,
        state: pi_coding_agent::core::extensions::types::ToolRenderState::new(),
        cwd: JsString::from("/repo"),
        execution_started: false,
        args_complete: true,
        is_partial: false,
        expanded,
        show_images,
        is_error,
    }
}

fn result(content: &[&str], details: Option<Value>) -> AgentToolResult {
    AgentToolResult {
        content: content
            .iter()
            .map(|text| TextOrImageContent::Text(TextContent::new(*text)))
            .collect(),
        details: details.map(JsValue::from),
        is_error: None,
        terminate: None,
    }
}

fn truncation(lines: usize) -> Value {
    json!({
        "content": "truncated",
        "truncated": true,
        "truncatedBy": "lines",
        "totalLines": lines,
        "totalBytes": 1000,
        "outputLines": 10,
        "outputBytes": 100,
        "lastLinePartial": false,
        "firstLineExceedsLimit": false,
        "maxLines": 2000,
        "maxBytes": 10240
    })
}

#[test]
fn coding_agent_tool_renderers_match_the_pinned_typescript_source() {
    let theme = dark_theme();
    let cases = vec![
        json!({"tool": "read", "kind": "call", "args": {"path": "src/file.txt"}}),
        json!({"tool": "read", "kind": "result", "args": {"path": "src/file.txt"},
            "result": {"content": [{"type": "text", "text": "line1\nline2\n\n"}]}}),
        json!({"tool": "edit", "kind": "call", "args": {"path": "src/file.txt", "edits": [{"oldText": "a", "newText": "b"}]}}),
        json!({"tool": "edit", "kind": "result",
            "args": {"path": "src/file.txt", "edits": [{"oldText": "a", "newText": "b"}]},
            "result": {"content": [{"type": "text", "text": "Successfully replaced 1 block(s) in src/file.txt."}],
                "details": {"diff": "@@ -1,1 +1,1 @@\n-a\n+b", "firstChangedLine": 1}}}),
        json!({"tool": "edit", "kind": "result", "isError": true,
            "args": {"path": "src/file.txt", "edits": [{"oldText": "a", "newText": "b"}]},
            "result": {"content": [{"type": "text", "text": "Could not edit file: src/file.txt. Error code: ENOENT."}]}}),
        json!({"tool": "write", "kind": "call", "args": {"path": "src/new.txt", "content": "hello\nworld\n"}}),
        json!({"tool": "write", "kind": "result", "isError": true,
            "result": {"content": [{"type": "text", "text": "disk full"}]}}),
        json!({"tool": "ls", "kind": "call", "args": {"path": "src", "limit": 5}}),
        json!({"tool": "ls", "kind": "result",
            "result": {"content": [{"type": "text", "text": "a\nb/"}], "details": {"entryLimitReached": 5}}}),
        json!({"tool": "grep", "kind": "call", "args": {"pattern": "foo", "path": "src", "glob": "*.ts", "limit": 3}}),
        json!({"tool": "grep", "kind": "result",
            "result": {"content": [{"type": "text", "text": "a.ts:1: foo"}], "details": {"linesTruncated": true}}}),
        json!({"tool": "find", "kind": "call", "args": {"pattern": "*.ts", "path": "src", "limit": 4}}),
        json!({"tool": "find", "kind": "result",
            "result": {"content": [{"type": "text", "text": "a.ts\nb.ts"}], "details": {"resultLimitReached": 4}}}),
        json!({"tool": "outline", "kind": "call", "args": {"path": "src", "digest": true, "glob": "**/*.ts", "show": "createTool"}}),
        json!({"tool": "outline", "kind": "result", "result": {"content": [{"type": "text", "text": "outline output"}]}}),
        json!({"tool": "symbol", "kind": "call", "args": {"name": "createTool", "kind": "function", "file": "src/core/tools"}}),
        json!({"tool": "references", "kind": "call", "args": {"name": "createTool", "kind": "call"}}),
        json!({"tool": "read", "kind": "result", "args": {"path": "src/file.txt"},
            "result": {"content": [{"type": "text", "text": "line1"}], "details": {"truncation": truncation(2500)}}}),
        json!({"tool": "bash", "kind": "result", "args": {"command": "printf"},
            "result": {"content": [{"type": "text", "text": "line1\nline2"}]}}),
        json!({"tool": "bash", "kind": "result", "expanded": false, "args": {"command": "seq"},
            "result": {"content": [{"type": "text", "text": "1\n2\n3\n4\n5\n6\n7\n8"}]}}),
        json!({"tool": "bash", "kind": "result", "args": {"command": "seq"},
            "result": {"content": [{"type": "text", "text": "last\n\n[Showing last 100B of 2KB. Full output: /tmp/pi-tool-test.log]"}],
                "details": {"truncation": truncation(2500), "fullOutputPath": "/tmp/pi-tool-test.log"}}}),
    ];

    let mut oracle = oracle::TsOracle::spawn_with_environment(
        "coding-agent-tool-renderers-oracle.ts",
        "packages/coding-agent/src/core/tools/read.ts",
        &[("COLORTERM", std::ffi::OsString::from("truecolor"))],
    );
    let expected = oracle.request(&json!({
        "id": 27_900,
        "operation": "coding_agent_tool_renderers",
        "cwd": "/repo",
        "cases": cases,
    }));
    assert_eq!(expected["ok"], Value::Bool(true), "{expected}");
    let expected_results = expected["output"].as_array().expect("oracle output array");
    assert_eq!(expected_results.len(), cases.len());
    for (index, (case, expected)) in cases.iter().zip(expected_results).enumerate() {
        let actual = render_case(case, &theme);
        assert_eq!(&actual, expected, "case {index}");
    }
}

#[test]
fn read_renderer_branch_matrix_matches_frozen() {
    let theme = dark_theme();
    let mut cases = vec![
        json!({"tool":"read", "kind":"call", "args":{"path":"file.txt", "offset":0, "limit":1}}),
        json!({"tool":"read", "kind":"call", "args":{"path":"file.txt", "offset":1.5, "limit":0.5}}),
        json!({"tool":"read", "kind":"call", "args":{"path":"AGENTS.override.md"}, "expanded":false}),
        json!({"tool":"read", "kind":"result", "args":{"path":"file.rs"}, "isError":true,
            "result":{"content":[{"type":"text", "text":"let error = true;"}]}}),
        json!({"tool":"read", "kind":"result", "args":{"path":"file.txt"}, "isError":true, "expanded":false,
            "result":{"content":[{"type":"text", "text":(1..=15).map(|line| line.to_string()).collect::<Vec<_>>().join("\n")}]}}),
    ];
    for path in [
        "AGENTS.md",
        "AGENTS.local.md",
        "AGENTS.MD",
        "AGENTS.local.MD",
        "CLAUDE.md",
        "CLAUDE.local.md",
        "CLAUDE.MD",
        "CLAUDE.local.MD",
        "skills/test/SKILL.md",
    ] {
        cases.push(json!({"tool":"read", "kind":"call", "args":{"path":path}, "expanded":false}));
    }
    for details in [
        json!({"truncation":{"truncated":true,"firstLineExceedsLimit":true}}),
        json!({"truncation":{"truncated":true,"truncatedBy":"lines","outputLines":10,"totalLines":30}}),
        json!({"truncation":{"truncated":true,"truncatedBy":"bytes","outputLines":10}}),
    ] {
        cases.push(json!({"tool":"read", "kind":"result", "args":{"path":"file.txt"},
            "result":{"content":[{"type":"text", "text":"head"}], "details":details}}));
    }
    let mut oracle = oracle::TsOracle::spawn_with_environment(
        "coding-agent-tool-renderers-oracle.ts",
        "packages/coding-agent/src/core/tools/read.ts",
        &[("COLORTERM", std::ffi::OsString::from("truecolor"))],
    );
    let expected = oracle.request(&json!({"id":27901,"cwd":"/repo","cases":cases}));
    assert_eq!(expected["ok"], json!(true), "{expected}");
    let mismatches = cases
        .iter()
        .zip(expected["output"].as_array().unwrap())
        .enumerate()
        .filter_map(|(index, (case, expected))| {
            let actual = render_case(case, &theme);
            (actual != *expected).then(|| format!("case {index}: {case}\nactual {actual}\nexpected {expected}"))
        })
        .collect::<Vec<_>>();
    assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
}

fn render_case(case: &Value, theme: &pi_coding_agent::modes::interactive::theme::Theme) -> Value {
    let tool = case["tool"].as_str().expect("tool");
    let kind = case["kind"].as_str().expect("kind");
    let args = case.get("args").cloned().unwrap_or_else(|| json!({}));
    let expanded = case.get("expanded").and_then(Value::as_bool).unwrap_or(true);
    let is_error = case.get("isError").and_then(Value::as_bool).unwrap_or(false);
    let show_images = case.get("showImages").and_then(Value::as_bool).unwrap_or(true);
    let context = context(args.clone(), expanded, is_error, show_images);
    let component = if kind == "call" {
        match tool {
            "read" => read_render_call(&JsValue::from(args.clone()), theme, &context).expect("fixture path renders"),
            "write" => write_render_call(&JsValue::from(args.clone()), theme, &context).expect("fixture path renders"),
            "edit" => edit_render_call(&JsValue::from(args.clone()), theme, &context).expect("edit renderer"),
            "ls" => ls_render_call(&JsValue::from(args.clone()), theme, &context).expect("fixture path renders"),
            "grep" => grep_render_call(&JsValue::from(args.clone()), theme, &context).expect("fixture path renders"),
            "find" => find_render_call(&JsValue::from(args.clone()), theme, &context).expect("fixture path renders"),
            "outline" => {
                outline_render_call(&JsValue::from(args.clone()), theme, &context).expect("fixture path renders")
            }
            "symbol" => {
                symbol_render_call(&JsValue::from(args.clone()), theme, &context).expect("fixture path renders")
            }
            "references" => {
                references_render_call(&JsValue::from(args.clone()), theme, &context).expect("fixture path renders")
            }
            "bash" => bash_render_call(&JsValue::from(args.clone()), theme, &context).expect("bash renderer"),
            other => panic!("unknown tool {other}"),
        }
    } else {
        let result_value = case.get("result").cloned().unwrap_or_else(|| json!({"content": []}));
        let content = result_value["content"]
            .as_array()
            .expect("content")
            .iter()
            .filter_map(|block| block.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>();
        let result = result(content.as_slice(), result_value.get("details").cloned());
        let options = pi_coding_agent::core::extensions::types::ToolRenderResultOptions {
            expanded,
            is_partial: false,
        };
        match tool {
            "read" => read_render_result(&result, &options, theme, &context).expect("read renderer"),
            "write" => write_render_result(&result, &options, theme, &context).expect("fixture result renders"),
            "edit" => edit_render_result(&result, &options, theme, &context).expect("edit renderer"),
            "bash" => pi_coding_agent::core::tools::bash::render_result(&result, &options, theme, &context)
                .expect("bash renderer"),
            "ls" => ls_render_result(&result, &options, theme, &context).expect("fixture result renders"),
            "grep" => grep_render_result(&result, &options, theme, &context).expect("grep renderer"),
            "find" => find_render_result(&result, &options, theme, &context).expect("fixture result renders"),
            "outline" | "symbol" | "references" => code_index_render_result(&result, &options, theme, &context),
            other => panic!("unknown tool {other}"),
        }
    };
    let lines = component.borrow_mut().render(80);
    json!({"ok": true, "value": lines})
}
