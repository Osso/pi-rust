#[path = "support/env_probe.rs"]
mod env_probe;

use crate::shared_oracle as oracle;

use pi_coding_agent::core::extensions::types::RenderShell;
use pi_coding_agent::core::tools::{
    ALL_TOOL_NAMES, DEFAULT_ACTIVE_TOOL_NAMES, ToolName, ToolsOptions, create_all_tool_definitions,
    create_coding_tool_definitions, create_read_only_tool_definitions, create_tool_definition,
};
use pi_js_compat::js_json::JsString;
use serde_json::{Value, json};

fn describe(name: ToolName, cwd: &str) -> Value {
    let definition = create_tool_definition(name, &JsString::from(cwd), ToolsOptions::default());
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

    let coding = create_coding_tool_definitions(&JsString::from(cwd), ToolsOptions::default())
        .into_iter()
        .map(|definition| definition.name)
        .collect::<Vec<_>>();
    assert_eq!(json!(coding), expected["coding"]);
    let read_only = create_read_only_tool_definitions(&JsString::from(cwd), ToolsOptions::default())
        .into_iter()
        .map(|definition| definition.name)
        .collect::<Vec<_>>();
    assert_eq!(json!(read_only), expected["readOnly"]);

    let all_keys = create_all_tool_definitions(&JsString::from(cwd), ToolsOptions::default())
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

    let coding: Vec<String> =
        pi_coding_agent::core::tools::create_coding_tools(&JsString::from(cwd), ToolsOptions::default())
            .into_iter()
            .map(|tool| tool.name().to_owned())
            .collect();
    assert_eq!(coding, vec!["read", "bash", "edit", "write"]);

    let read_only: Vec<String> =
        pi_coding_agent::core::tools::create_read_only_tools(&JsString::from(cwd), ToolsOptions::default())
            .into_iter()
            .map(|tool| tool.name().to_owned())
            .collect();
    assert_eq!(
        read_only,
        vec!["read", "grep", "find", "ls", "outline", "symbol", "references"]
    );

    let all: Vec<String> =
        pi_coding_agent::core::tools::create_all_tools(&JsString::from(cwd), ToolsOptions::default())
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

#[tokio::test]
async fn standalone_tool_factories_preserve_undefined_context() {
    let mut oracle = oracle::TsOracle::spawn(
        "coding-agent-tools-index-oracle.ts",
        "packages/coding-agent/src/core/tools/index.ts",
    );
    let cases = [
        (ToolName::EndTurn, json!({"reason": "done"})),
        (ToolName::ChangeWorkingDirectory, json!({"path": "/tmp"})),
        (ToolName::ResumeSession, json!({"path": "/tmp/session.jsonl"})),
        (ToolName::SearchCurrentSessionHistory, json!({"query": "needle"})),
        (ToolName::ListSessions, json!({})),
        (ToolName::Broadcast, json!({"message": "fixture"})),
        (ToolName::ChannelPost, json!({"message": "fixture"})),
        (
            ToolName::AskQuestions,
            json!({"blocking": true, "questions": [{"question": "Q?"}]}),
        ),
        (ToolName::AskSupervisor, json!({"question": "Q?"})),
    ];
    for (name, params) in cases {
        let expected = oracle
            .request(&json!({"id": 4, "operation": "execute", "cwd": "/tmp", "name": name.as_str(), "params": params}));
        assert_eq!(expected["ok"], json!(true), "{expected}");
        let tool = pi_coding_agent::core::tools::create_tool(name, &JsString::from("/tmp"), ToolsOptions::default());
        let actual = tool
            .execute(
                "call".into(),
                pi_js_compat::js_json::JsValue::from(params),
                None,
                std::sync::Arc::new(|_| Ok(())),
                None,
            )
            .await;
        let actual = match actual {
            Ok(result) => json!({"result": result}),
            Err(error) => json!({"error": error.message}),
        };
        assert_eq!(actual, expected["output"], "{name}");
    }
}

#[test]
fn executable_availability_filters_only_noninjected_code_index_tools() {
    use std::os::unix::fs::PermissionsExt;
    let temp = tempfile::tempdir().unwrap();
    for (directory, permission) in [("missing", None), ("nonexec", Some(0o644)), ("executable", Some(0o755))] {
        let path = temp.path().join(directory);
        std::fs::create_dir(&path).unwrap();
        std::os::unix::fs::symlink("/usr/bin/git", path.join("git")).unwrap();
        if let Some(permission) = permission {
            let binary = path.join("code-index");
            std::fs::write(&binary, "fixture, never executed").unwrap();
            std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(permission)).unwrap();
        }
        env_probe::run_probe(
            "availability_probe",
            &[
                ("PATH", path.into_os_string()),
                ("PI_Q5_ORACLE_PATH", std::env::var_os("PATH").unwrap()),
            ],
        );
    }
}

#[test]
#[ignore = "controlled PATH probe"]
fn availability_probe() {
    if !env_probe::in_probe() {
        return;
    }
    let path = std::env::var("PATH").unwrap();
    let mut oracle = oracle::TsOracle::spawn_with_environment(
        "coding-agent-tools-index-oracle.ts",
        "packages/coding-agent/src/core/tools/index.ts",
        &[("PATH", std::env::var_os("PI_Q5_ORACLE_PATH").unwrap())],
    );
    for injected in [false, true] {
        let expected = oracle
            .request(&json!({"id": 5, "operation": "availability", "cwd": "/tmp", "path": path, "injected": injected}));
        assert_eq!(expected["ok"], json!(true), "{expected}");
        let options = ToolsOptions {
            code_index: injected.then(|| pi_coding_agent::core::tools::code_index::CodeIndexToolOptions {
                operations: Some(std::sync::Arc::new(
                    pi_coding_agent::core::tools::code_index::LocalCodeIndexOperations,
                )),
            }),
            ..Default::default()
        };
        let actual = pi_coding_agent::core::tools::create_tool_definitions_for_available_executables(
            &JsString::from("/tmp"),
            options,
        )
        .unwrap()
        .into_iter()
        .map(|(name, _)| name.as_str())
        .collect::<Vec<_>>();
        assert_eq!(json!(actual), expected["output"], "injected={injected}");
    }
}

// Shared support is compiled once per integration target.
#[path = "support/oracle.rs"]
pub mod shared_oracle;
