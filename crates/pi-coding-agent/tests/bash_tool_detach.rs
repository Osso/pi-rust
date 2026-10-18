//! Bash tool spawn context and detach wiring tests.

use std::collections::BTreeMap;
use std::sync::Arc;

use pi_agent_core::agent::{AgentTool, AgentToolUpdateCallback};
use pi_coding_agent::core::tools::bash::{BashSessionEnvironment, BashSpawnContext, BashTool, BashToolOptions};

fn text_of(result: &pi_agent_core::agent::AgentToolResult) -> String {
    result
        .content
        .iter()
        .filter_map(|content| match content {
            pi_ai::types::TextOrImageContent::Text(text) => Some(text.text.clone()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn run(tool: &BashTool, command: &str) -> String {
    let update: AgentToolUpdateCallback = Arc::new(|_| {});
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    let result = runtime
        .block_on(tool.execute(
            "tool-call".to_owned(),
            serde_json::json!({"command": command}),
            None,
            update,
            None,
        ))
        .expect("execute");
    text_of(&result)
}

#[test]
fn exposes_session_metadata_as_pi_environment_variables() {
    let tool = BashTool::with_options(
        std::env::current_dir().expect("cwd").to_string_lossy(),
        BashToolOptions {
            session_environment: Some(Arc::new(|| {
                Some(BashSessionEnvironment {
                    session_id: "session-1".to_owned(),
                    session_file: Some("/tmp/session.jsonl".to_owned()),
                    provider: Some("faux".to_owned()),
                    model: Some("faux-model".to_owned()),
                    reasoning_level: Some("high".to_owned()),
                })
            })),
            ..BashToolOptions::default()
        },
    );
    let output = run(
        &tool,
        "printf '%s|%s|%s|%s|%s' \"$PI_SESSION_ID\" \"$PI_SESSION_FILE\" \"$PI_PROVIDER\" \"$PI_MODEL\" \"$PI_REASONING_LEVEL\"",
    );
    assert_eq!(output, "session-1|/tmp/session.jsonl|faux|faux-model|high");
}

#[test]
fn omits_session_metadata_when_session_environment_is_not_exposed() {
    let tool = BashTool::with_options(
        std::env::current_dir().expect("cwd").to_string_lossy(),
        BashToolOptions {
            expose_session_environment: Some(false),
            session_environment: Some(Arc::new(|| {
                Some(BashSessionEnvironment {
                    session_id: "session-1".to_owned(),
                    ..BashSessionEnvironment::default()
                })
            })),
            ..BashToolOptions::default()
        },
    );
    let output = run(&tool, "printf '%s' \"${PI_SESSION_ID-unset}\"");
    assert_eq!(output, "unset");
}

#[test]
fn applies_the_spawn_hook_to_command_cwd_and_environment() {
    let tool = BashTool::with_options(
        std::env::current_dir().expect("cwd").to_string_lossy(),
        BashToolOptions {
            spawn_hook: Some(Arc::new(|context: BashSpawnContext| BashSpawnContext {
                command: format!("printf '%s:%s' \"$HOOK_VALUE\" {}", context.command),
                cwd: context.cwd,
                env: {
                    let mut env: BTreeMap<String, String> = context.env;
                    env.insert("HOOK_VALUE".to_owned(), "hooked".to_owned());
                    env
                },
            })),
            ..BashToolOptions::default()
        },
    );
    let output = run(&tool, "ignored");
    assert_eq!(output, "hooked:ignored");
}
