#[path = "support/oracle.rs"]
mod oracle;

use std::sync::{Arc, Mutex};

use pi_agent_core::AgentToolResult;
use pi_ai::request::BoxFuture;
use pi_coding_agent::cli::list_extensions::{format_extension_inventory, list_extensions};
use pi_coding_agent::core::event_bus::EventBusController;
use pi_coding_agent::core::extensions::types::{
    Extension, ExtensionContext, HandlerError, SessionStartEvent, ToolDefinition,
};
use pi_coding_agent::core::extensions::{ExtensionFactory, create_extension_runtime, load_extension_from_factory};
use pi_js_compat::js_json::JsString;
use serde_json::{Value, json};

fn text(value: &JsString) -> String {
    value.to_utf8().expect("oracle fixtures are well-formed")
}

struct ExtensionSpec {
    path: &'static str,
    source: &'static str,
    scope: &'static str,
    commands: usize,
    tools: usize,
    handlers: usize,
}

async fn build_extension(spec: &ExtensionSpec) -> Extension {
    let runtime = create_extension_runtime();
    let setup = Mutex::new(Some((spec.commands, spec.tools, spec.handlers)));
    let factory: ExtensionFactory = Arc::new(move |api| {
        let (commands, tools, handlers) = setup
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
            .expect("extension setup runs once");
        for index in 0..commands {
            api.register_command(
                &format!("command{index}"),
                None,
                None,
                Arc::new(|_args, _ctx| Box::pin(async { Ok(()) })),
            )
            .unwrap();
        }
        for index in 0..tools {
            api.register_tool(tool_definition(&format!("tool{index}"))).unwrap();
        }
        for _ in 0..handlers {
            api.on_session_start(|_event: SessionStartEvent, _ctx: ExtensionContext| {
                Box::pin(async { Ok(None) }) as BoxFuture<'static, Result<Option<()>, HandlerError>>
            })
            .unwrap();
        }
        Box::pin(async { Ok(()) })
    });
    let extension = load_extension_from_factory(
        factory,
        &JsString::from("/tmp"),
        Arc::new(EventBusController::new()),
        runtime,
        &JsString::from(spec.path),
    )
    .await
    .expect("extension factory succeeds");
    extension.read().source_info.scope = match spec.scope {
        "user" => pi_coding_agent::core::source_info::SourceScope::User,
        "project" => pi_coding_agent::core::source_info::SourceScope::Project,
        "temporary" => pi_coding_agent::core::source_info::SourceScope::Temporary,
        other => panic!("unknown scope {other}"),
    };
    extension.read().source_info.source = spec.source.into();
    extension
}

fn tool_definition(name: &str) -> ToolDefinition {
    ToolDefinition {
        name: name.to_owned(),
        label: name.into(),
        description: name.into(),
        prompt_snippet: None,
        prompt_guidelines: None,
        approval_required: None,
        approval_kind: None,
        parameters: json!({"type": "object"}),
        render_shell: None,
        prepare_arguments: None,
        prepare_execution: None,
        execution_mode: None,
        execute: Arc::new(|_id, _params, _signal, _on_update, _ctx| {
            Box::pin(async {
                Ok(AgentToolResult {
                    content: Vec::new(),
                    details: None,
                    is_error: None,
                    terminate: None,
                })
            })
        }),
        render_call: None,
        render_result: None,
    }
}

fn wire(extensions: &[Extension]) -> Value {
    Value::Array(
        extensions
            .iter()
            .map(|extension| {
                let registration = extension.read();
                json!({
                    "path": text(&registration.path),
                    "sourceInfo": {
                        "scope": registration.source_info.scope.as_str(),
                        "source": text(&registration.source_info.source),
                    },
                    "commands": registration.commands.len(),
                    "tools": registration.tools.len(),
                    "handlers": registration.handlers.len(),
                })
            })
            .collect(),
    )
}

fn specs(case: usize) -> Vec<ExtensionSpec> {
    match case {
        0 => Vec::new(),
        1 => vec![
            ExtensionSpec {
                path: "/ext/builtin/plan.ts",
                source: "builtin",
                scope: "user",
                commands: 2,
                tools: 1,
                handlers: 3,
            },
            ExtensionSpec {
                path: "<first-party:status>",
                source: "ignored",
                scope: "project",
                commands: 0,
                tools: 0,
                handlers: 1,
            },
            ExtensionSpec {
                path: "/home/user/.pi/agent/extensions/notes/index.ts",
                source: "package:@scope/notes",
                scope: "project",
                commands: 12,
                tools: 4,
                handlers: 0,
            },
        ],
        2 => vec![
            ExtensionSpec {
                path: "/z/zeta.ts",
                source: "user",
                scope: "user",
                commands: 1,
                tools: 0,
                handlers: 0,
            },
            ExtensionSpec {
                path: "/a/Alpha.ts",
                source: "user",
                scope: "user",
                commands: 0,
                tools: 1,
                handlers: 0,
            },
            ExtensionSpec {
                path: "/a/alpha.ts",
                source: "user",
                scope: "user",
                commands: 0,
                tools: 0,
                handlers: 1,
            },
            ExtensionSpec {
                path: "/a/éclair.ts",
                source: "temporary",
                scope: "temporary",
                commands: 0,
                tools: 0,
                handlers: 0,
            },
        ],
        _ => vec![
            ExtensionSpec {
                path: "<first-party:very-long-first-party-extension>",
                source: "x",
                scope: "user",
                commands: 100,
                tools: 23,
                handlers: 9,
            },
            ExtensionSpec {
                path: "/e/日本語.ts",
                source: "package:日本語",
                scope: "project",
                commands: 0,
                tools: 1,
                handlers: 0,
            },
        ],
    }
}

#[tokio::test]
async fn list_extensions_matches_the_pinned_typescript_source() {
    let mut oracle = oracle::TsOracle::spawn(
        "list-extensions-oracle.ts",
        "packages/coding-agent/src/cli/list-extensions.ts",
    );

    for case in 0..4 {
        let specs = specs(case);
        let mut extensions = Vec::new();
        for spec in &specs {
            extensions.push(build_extension(spec).await);
        }
        let expected = oracle.request(&json!({
            "id": 28_600 + case,
            "operation": "format",
            "extensions": wire(&extensions),
        }));
        let actual = json!({
            "id": 28_600 + case,
            "ok": true,
            "output": text(&format_extension_inventory(&extensions)),
        });
        assert_eq!(expected, actual, "list extensions mismatch: case {case}");
    }
}

#[tokio::test]
async fn list_extensions_appends_the_console_log_newline() {
    let spec = ExtensionSpec {
        path: "/ext/read.ts",
        source: "builtin",
        scope: "user",
        commands: 1,
        tools: 0,
        handlers: 0,
    };
    let extension = build_extension(&spec).await;
    let extensions = vec![extension];
    assert_eq!(
        list_extensions(&extensions),
        format_extension_inventory(&extensions) + "\n"
    );
}

#[test]
fn no_extensions_message() {
    assert!(format_extension_inventory(&[]) == *"Loaded extensions: none");
}

#[tokio::test]
async fn inventory_keeps_lone_surrogate_extension_basenames() {
    let extension = build_extension(&ExtensionSpec {
        path: "/ext/plain.ts",
        source: "user",
        scope: "user",
        commands: 0,
        tools: 0,
        handlers: 0,
    })
    .await;
    let mut path = JsString::from("/ext/");
    path.push_str(JsString::from_utf16(vec![0xD83D]));
    path.push_str(".ts");
    extension.read().path = path;

    let output = format_extension_inventory(&[extension]);

    let mut expected_label = JsString::from_utf16(vec![0xD83D]);
    expected_label.push_str(".ts");
    assert!(output.contains(expected_label));
}
