#[path = "support/oracle.rs"]
mod oracle;

use pi_coding_agent::core::extensions::types::RenderShell;
use pi_coding_agent::core::tools::{
    ALL_TOOL_NAMES, DEFAULT_ACTIVE_TOOL_NAMES, ToolName, ToolsOptions, create_all_tool_definitions,
    create_coding_tool_definitions, create_read_only_tool_definitions, create_tool_definition,
};
use serde_json::{Value, json};

fn describe(name: ToolName, cwd: &str) -> Value {
    let definition = create_tool_definition(name, cwd, ToolsOptions::default());
    json!({
        "name": definition.name,
        "label": definition.label,
        "description": definition.description,
        "promptSnippet": definition.prompt_snippet,
        "promptGuidelines": definition.prompt_guidelines,
        "approvalRequired": definition.approval_required,
        "approvalKind": definition.approval_kind.map(|kind| match kind {
            pi_coding_agent::core::extensions::types::ApprovalKind::ReadOnly => "read-only",
        }),
        "parameters": definition.parameters,
        "executionMode": definition.execution_mode.map(|mode| mode.as_str()),
        "renderShell": definition.render_shell.map(|shell| match shell {
            RenderShell::Default => "default",
            RenderShell::SelfRender => "self",
        }),
        "hasRenderCall": definition.render_call.is_some(),
        "hasRenderResult": definition.render_result.is_some(),
    })
}

fn first_difference(path: &str, actual: &Value, expected: &Value) -> Option<String> {
    match (actual, expected) {
        (Value::Object(actual), Value::Object(expected)) => {
            for (key, value) in actual {
                let Some(expected) = expected.get(key) else {
                    return Some(format!("{path}.{key}: unexpected {value}"));
                };
                if let Some(difference) = first_difference(&format!("{path}.{key}"), value, expected) {
                    return Some(difference);
                }
            }
            expected
                .keys()
                .find(|key| !actual.contains_key(*key))
                .map(|key| format!("{path}.{key}: missing"))
        }
        (Value::Array(actual), Value::Array(expected)) => {
            if actual.len() != expected.len() {
                return Some(format!("{path}: length {} != {}", actual.len(), expected.len()));
            }
            actual
                .iter()
                .zip(expected)
                .enumerate()
                .find_map(|(index, (actual, expected))| first_difference(&format!("{path}[{index}]"), actual, expected))
        }
        _ => (actual != expected).then(|| format!("{path}: {actual} != {expected}")),
    }
}

#[test]
fn tools_index_factories_match_the_pinned_typescript_source() {
    let cwd = "/tmp/pi-tools-index-differential";
    let mut oracle = oracle::TsOracle::spawn(
        "coding-agent-tools-index-oracle.ts",
        "packages/coding-agent/src/core/tools/index.ts",
    );
    let expected = oracle.request(&json!({
        "id": 27_960,
        "operation": "coding_agent_tools_index",
        "cwd": cwd,
    }));
    assert_eq!(expected["ok"], Value::Bool(true), "{expected}");
    let expected = &expected["output"];

    let names = DEFAULT_ACTIVE_TOOL_NAMES
        .iter()
        .map(|name| name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(json!(names), expected["names"]);
    let all_names = ALL_TOOL_NAMES.iter().map(|name| name.as_str()).collect::<Vec<_>>();
    assert_eq!(json!(all_names), expected["allNames"]);

    let definitions = DEFAULT_ACTIVE_TOOL_NAMES
        .iter()
        .map(|name| describe(*name, cwd))
        .collect::<Vec<_>>();
    let expected_definitions = expected["definitions"].as_array().expect("definition array");
    for (index, actual) in definitions.iter().enumerate() {
        if let Some(difference) = first_difference(names[index], actual, &expected_definitions[index]) {
            panic!("definition {index} mismatch: {difference}");
        }
    }

    let coding = create_coding_tool_definitions(cwd, ToolsOptions::default())
        .into_iter()
        .map(|definition| definition.name)
        .collect::<Vec<_>>();
    assert_eq!(json!(coding), expected["coding"]);
    let read_only = create_read_only_tool_definitions(cwd, ToolsOptions::default())
        .into_iter()
        .map(|definition| definition.name)
        .collect::<Vec<_>>();
    assert_eq!(json!(read_only), expected["readOnly"]);

    let all_keys = create_all_tool_definitions(cwd, ToolsOptions::default())
        .into_iter()
        .map(|(name, definition)| {
            assert_eq!(name.as_str(), definition.name);
            name.as_str().to_owned()
        })
        .collect::<Vec<_>>();
    assert_eq!(json!(all_keys), expected["allKeys"]);
}

#[test]
fn tool_name_round_trips_the_source_names() {
    for name in ToolName::ALL {
        assert_eq!(ToolName::from_name(name.as_str()), Some(name));
    }
    assert_eq!(ToolName::from_name("unknown"), None);
}

#[test]
fn tool_factories_return_the_source_names_in_order() {
    let cwd = "/tmp/pi-tools-index-differential";
    let context_factory: pi_coding_agent::core::tools::tool_definition_wrapper::ToolContextFactory =
        std::sync::Arc::new(|| unreachable!("tool creation must not request a context"));

    let coding: Vec<String> =
        pi_coding_agent::core::tools::create_coding_tools(cwd, ToolsOptions::default(), &context_factory)
            .into_iter()
            .map(|tool| tool.name().to_owned())
            .collect();
    assert_eq!(coding, vec!["read", "bash", "edit", "write"]);

    let read_only: Vec<String> =
        pi_coding_agent::core::tools::create_read_only_tools(cwd, ToolsOptions::default(), &context_factory)
            .into_iter()
            .map(|tool| tool.name().to_owned())
            .collect();
    assert_eq!(
        read_only,
        vec!["read", "grep", "find", "ls", "outline", "symbol", "references"]
    );

    let all: Vec<String> =
        pi_coding_agent::core::tools::create_all_tools(cwd, ToolsOptions::default(), &context_factory)
            .into_iter()
            .map(|(name, tool)| {
                assert_eq!(name.as_str(), tool.name());
                name.as_str().to_owned()
            })
            .collect();
    assert_eq!(all.len(), ToolName::ALL.len());
    assert_eq!(
        all,
        ALL_TOOL_NAMES
            .iter()
            .map(|name| name.as_str().to_owned())
            .collect::<Vec<_>>()
    );
}
