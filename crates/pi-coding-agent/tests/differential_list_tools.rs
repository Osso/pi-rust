use crate::shared_oracle as oracle;

use pi_coding_agent::cli::list_tools::{format_tool_inventory, list_tools};
use pi_coding_agent::core::extensions::types::ToolInfo;
use pi_coding_agent::core::source_info::{SourceInfo, SourceOrigin, SourceScope};
use pi_js_compat::js_json::{JsString, encode_node_utf8};
use serde_json::{Value, json};

fn tool(name: &str, description: &str, source: &str, path: &str) -> ToolInfo {
    ToolInfo {
        name: name.to_owned(),
        description: description.into(),
        parameters: json!({"type": "object"}),
        prompt_guidelines: None,
        source_info: SourceInfo {
            path: path.into(),
            source: source.into(),
            scope: SourceScope::User,
            origin: SourceOrigin::TopLevel,
            base_dir: None,
        },
    }
}

fn wire(tools: &[ToolInfo]) -> Value {
    Value::Array(tools.iter().map(|tool| serde_json::to_value(tool).unwrap()).collect())
}

struct Case {
    id: u64,
    name: &'static str,
    tools: Vec<ToolInfo>,
    active: Vec<&'static str>,
}

fn cases() -> Vec<Case> {
    vec![
        Case {
            id: 28_500,
            name: "no tools",
            tools: Vec::new(),
            active: Vec::new(),
        },
        Case {
            id: 28_501,
            name: "sources and active flags",
            tools: vec![
                tool("read", "Read file contents", "builtin", "<builtin:read>"),
                tool("grep", "Search file contents", "builtin", "<builtin:grep>"),
                tool("plan", "Plan mode helpers", "extension:/ext/plan.ts", "/ext/plan.ts"),
                tool("sdk_tool", "Provided by the SDK", "sdk", "<sdk>"),
                tool("pkg_tool", "From a package", "package:@scope/pkg", "/pkg/index.ts"),
            ],
            active: vec!["read", "plan"],
        },
        Case {
            id: 28_502,
            name: "description truncation",
            tools: vec![
                tool("exact", &"a".repeat(54), "builtin", "<builtin:exact>"),
                tool("long", &"b".repeat(55), "builtin", "<builtin:long>"),
                tool(
                    "padded",
                    &format!("{}   ", "c".repeat(51)),
                    "builtin",
                    "<builtin:padded>",
                ),
                tool(
                    "wide",
                    "日本語の説明文がとても長い場合の切り詰め動作を確認するための説明文です。ここまで長いと切り詰められます",
                    "builtin",
                    "<builtin:wide>",
                ),
            ],
            active: Vec::new(),
        },
        Case {
            id: 28_503,
            name: "locale name ordering",
            tools: vec![
                tool("zeta", "z", "builtin", "<builtin:zeta>"),
                tool("Alpha", "a", "builtin", "<builtin:Alpha>"),
                tool("alpha", "a", "builtin", "<builtin:alpha>"),
                tool("éclair", "e", "builtin", "<builtin:eclair>"),
            ],
            active: vec!["alpha"],
        },
    ]
}

#[test]
fn list_tools_matches_the_pinned_typescript_source() {
    let mut oracle = oracle::TsOracle::spawn("list-tools-oracle.ts", "packages/coding-agent/src/cli/list-tools.ts");

    for case in cases() {
        let active: Vec<String> = case.active.iter().map(|name| (*name).to_owned()).collect();
        let expected = oracle.request(&json!({
            "id": case.id,
            "operation": "format",
            "tools": wire(&case.tools),
            "activeToolNames": active,
        }));
        let actual = json!({
            "id": case.id,
            "ok": true,
            "output": encode_node_utf8(&format_tool_inventory(&case.tools, &active)),
        });
        assert_eq!(expected, actual, "list tools mismatch: {}", case.name);
    }
}

#[test]
fn astral_truncation_keeps_the_split_surrogate_until_stdout() {
    // JavaScript `slice(0, 51)` leaves a lone high surrogate; Node encodes it as
    // U+FFFD only when the value is written.
    let tools = vec![tool(
        "astral",
        &format!("{}\u{1F600}tail", "d".repeat(50)),
        "builtin",
        "<builtin:astral>",
    )];
    let mut truncated = JsString::from("d".repeat(50));
    truncated.push_unit(0xD83D);
    truncated.push_str("...");
    assert!(format_tool_inventory(&tools, &[]).contains(truncated));
    assert!(encode_node_utf8(&list_tools(&tools, &[])).contains(&format!("{}\u{fffd}...", "d".repeat(50))));
}

#[test]
fn list_tools_appends_the_console_log_newline() {
    let tools = vec![tool("read", "Read file contents", "builtin", "<builtin:read>")];
    assert_eq!(
        list_tools(&tools, &["read".to_owned()]),
        format_tool_inventory(&tools, &["read".to_owned()]) + "\n"
    );
}
