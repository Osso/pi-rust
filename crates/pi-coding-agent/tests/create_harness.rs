//! Behavioral tests for `server/create-harness.ts` prompt assembly.

use std::sync::Arc;

use pi_agent_core::agent::types::{AgentTool, AgentToolResult, AgentToolUpdateCallback, ToolError};
use pi_agent_core::harness::agent_harness::HarnessTool;
use pi_agent_core::harness::types::AgentToolExecutionContext;
use pi_ai::request::BoxFuture;
use pi_coding_agent::core::system_prompt::BuildSystemPromptOptions;
use pi_coding_agent::server::create_harness::{
    BuildCodingAgentHarnessSystemPromptOptions, CodingAgentHarnessTool, build_coding_agent_harness_system_prompt,
};
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

struct FakeTool {
    name: String,
    description: String,
    parameters: Value,
    label: String,
}

impl AgentTool for FakeTool {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn parameters(&self) -> &Value {
        &self.parameters
    }

    fn label(&self) -> &str {
        &self.label
    }

    fn execute(
        &self,
        _tool_call_id: String,
        _params: Value,
        _signal: Option<CancellationToken>,
        _on_update: AgentToolUpdateCallback,
        _execution: Option<AgentToolExecutionContext>,
    ) -> BoxFuture<'static, Result<AgentToolResult, ToolError>> {
        Box::pin(async { Ok(AgentToolResult::text("ok")) })
    }
}

fn tool(name: &str, snippet: Option<&str>, guidelines: &[&str]) -> CodingAgentHarnessTool {
    CodingAgentHarnessTool::new(
        HarnessTool::new(Arc::new(FakeTool {
            name: name.to_owned(),
            description: format!("{name} tool"),
            parameters: json!({"type": "object"}),
            label: name.to_owned(),
        })),
        snippet.map(str::to_owned),
        guidelines.iter().map(|guideline| (*guideline).to_owned()).collect(),
    )
}

#[test]
fn active_tools_contribute_collapsed_snippets_and_guidelines_in_order() {
    let tools = vec![
        tool("read", Some("Read file\ncontents"), &["Use read to examine files."]),
        tool("bash", Some("Execute bash   commands"), &["Inspect PI_* variables."]),
        tool("write", None, &[]),
    ];
    let active = vec!["bash".to_owned(), "read".to_owned(), "write".to_owned()];

    let prompt = build_coding_agent_harness_system_prompt(BuildCodingAgentHarnessSystemPromptOptions {
        cwd: "/workspace",
        tools: &tools,
        active_tool_names: &active,
        system_prompt_options: None,
    });

    assert!(prompt.contains("Execute bash commands"));
    assert!(prompt.contains("Read file contents"));
    let bash_index = prompt.find("Execute bash commands").unwrap();
    let read_index = prompt.find("Read file contents").unwrap();
    assert!(bash_index < read_index, "active-tool order drives snippet order");
    let guideline_bash = prompt.find("Inspect PI_* variables.").unwrap();
    let guideline_read = prompt.find("Use read to examine files.").unwrap();
    assert!(guideline_bash < guideline_read);
    assert!(prompt.contains("/workspace"));
}

#[test]
fn inactive_tools_are_excluded_and_blank_snippets_are_skipped() {
    let tools = vec![
        tool("read", Some("Read file contents"), &["read guideline"]),
        tool("bash", Some("   \n  "), &["bash guideline"]),
    ];
    let active = vec!["bash".to_owned()];

    let prompt = build_coding_agent_harness_system_prompt(BuildCodingAgentHarnessSystemPromptOptions {
        cwd: "/workspace",
        tools: &tools,
        active_tool_names: &active,
        system_prompt_options: Some(&BuildSystemPromptOptions {
            cwd: "/ignored".to_owned(),
            append_system_prompt: Some("appended".to_owned()),
            ..Default::default()
        }),
    });

    assert!(!prompt.contains("Read file contents"));
    assert!(!prompt.contains("read guideline"));
    assert!(prompt.contains("bash guideline"));
    assert!(prompt.contains("appended"));
    assert!(prompt.contains("/workspace"));
    assert!(!prompt.contains("/ignored"));
}

fn harness_session(
    id: &str,
) -> pi_agent_core::harness::session::Session<pi_agent_core::harness::session::types::SessionMetadata> {
    use pi_agent_core::harness::session::types::SessionMetadata;
    use pi_agent_core::harness::session::{InMemorySessionStorage, Session};
    Session::new(Arc::new(InMemorySessionStorage::new(SessionMetadata {
        id: id.to_owned(),
        created_at: 1,
        parent_session_id: None,
    })))
}

fn builtin_model() -> pi_ai::types::Model {
    pi_ai::providers::all::get_builtin_model("google", "gemini-2.5-flash").expect("builtin model")
}

fn harness_options(
    session: pi_agent_core::harness::session::Session<pi_agent_core::harness::session::types::SessionMetadata>,
) -> pi_agent_core::harness::agent_harness::AgentHarnessOptions {
    pi_agent_core::harness::agent_harness::AgentHarnessOptions::new(
        session,
        Arc::new(pi_ai::models::create_models(
            pi_ai::models::CreateModelsOptions::default(),
        )),
        builtin_model(),
    )
}

fn node_env(
    cwd: &str,
    shell_env: std::collections::HashMap<String, String>,
) -> Arc<dyn pi_agent_core::harness::types::ExecutionEnv> {
    Arc::new(pi_agent_core::harness::env::NodeExecutionEnv::new(
        pi_agent_core::harness::env::NodeExecutionEnvOptions {
            cwd: cwd.to_owned(),
            shell_path: None,
            shell_env,
        },
    ))
}

async fn execute_tool(tool: pi_agent_core::harness::agent_harness::HarnessTool, params: Value) -> Value {
    let result = tool
        .tool
        .execute("call".to_owned(), params, None, Arc::new(|_update| {}), None)
        .await
        .expect("tool result");
    serde_json::to_value(result.content).expect("content json")
}

#[tokio::test]
async fn default_tools_and_explicit_harness_options_are_applied() {
    let env = node_env("/workspace", Default::default());
    let mut options = harness_options(harness_session("harness-session"));
    options.thinking_level = Some(pi_agent_core::harness::types::ThinkingLevel::High);
    options.stream_options = Some(pi_ai::request::SimpleStreamOptions {
        stream: pi_ai::request::StreamOptions {
            max_tokens: Some(123.0),
            ..Default::default()
        },
        ..Default::default()
    });
    options.retry = Some(pi_ai::utils::retry::RetryPolicy {
        enabled: true,
        max_retries: 2,
        base_delay_ms: 10.0,
    });
    options.steering_mode = Some(pi_agent_core::harness::types::QueueMode::All);
    options.follow_up_mode = Some(pi_agent_core::harness::types::QueueMode::All);
    let created = pi_coding_agent::server::create_harness::create_coding_agent_harness(
        pi_coding_agent::server::create_harness::CreateCodingAgentHarnessOptions {
            harness: options,
            env,
            bash_command_prefix: None,
            session_file: None,
            tools: None,
            system_prompt_options: None,
        },
    )
    .await
    .expect("harness");
    assert!(created.suspended.is_empty());
    assert_eq!(
        created.harness.get_active_tools().await,
        vec!["read", "bash", "edit", "write"]
    );
    let names: Vec<String> = created
        .harness
        .get_tools()
        .await
        .iter()
        .map(|tool| tool.name().to_owned())
        .collect();
    assert_eq!(names, vec!["read", "bash", "edit", "write"]);
    assert_eq!(
        created.harness.get_stream_options().await.stream.max_tokens,
        Some(123.0)
    );
    assert_eq!(created.harness.get_retry_policy().await.max_retries, 2);
    assert_eq!(
        created.harness.get_steering_mode().await,
        pi_agent_core::harness::types::QueueMode::All
    );
    assert_eq!(
        created.harness.get_follow_up_mode().await,
        pi_agent_core::harness::types::QueueMode::All
    );
}

#[tokio::test]
async fn caller_supplied_tools_and_activation_are_preserved() {
    let env = node_env("/workspace", Default::default());
    let custom = tool(
        "inspect",
        Some("Inspect the service"),
        &["Use inspect for diagnostics."],
    );
    let mut options = harness_options(harness_session("custom-harness-session"));
    options.active_tool_names = Some(Vec::new());
    options.system_prompt = Some(pi_agent_core::harness::agent_harness::SystemPromptSource::Static(
        "Server-owned prompt".to_owned(),
    ));
    let created = pi_coding_agent::server::create_harness::create_coding_agent_harness(
        pi_coding_agent::server::create_harness::CreateCodingAgentHarnessOptions {
            harness: options,
            env,
            bash_command_prefix: None,
            session_file: None,
            tools: Some(vec![custom]),
            system_prompt_options: None,
        },
    )
    .await
    .expect("harness");
    let names: Vec<String> = created
        .harness
        .get_tools()
        .await
        .iter()
        .map(|tool| tool.name().to_owned())
        .collect();
    assert_eq!(names, vec!["inspect"]);
    assert!(created.harness.get_active_tools().await.is_empty());
}

#[tokio::test]
async fn bash_receives_the_session_file_and_current_model_state() {
    let mut shell_env = std::collections::HashMap::new();
    shell_env.insert("PI_SESSION_FILE".to_owned(), "/stale/parent.jsonl".to_owned());
    shell_env.insert("PI_CODING_AGENT".to_owned(), "true".to_owned());
    let cwd = std::env::current_dir().expect("cwd").display().to_string();
    let env = node_env(&cwd, shell_env);
    let mut options = harness_options(harness_session("session-file-harness"));
    options.thinking_level = Some(pi_agent_core::harness::types::ThinkingLevel::High);
    let created = pi_coding_agent::server::create_harness::create_coding_agent_harness(
        pi_coding_agent::server::create_harness::CreateCodingAgentHarnessOptions {
            harness: options,
            env,
            bash_command_prefix: None,
            session_file: Some("/sessions/current.jsonl".to_owned()),
            tools: None,
            system_prompt_options: None,
        },
    )
    .await
    .expect("harness");
    let bash = created
        .harness
        .get_tools()
        .await
        .into_iter()
        .find(|tool| tool.name() == "bash")
        .expect("bash tool");
    let content = execute_tool(
        bash,
        json!({
            "command": "printf '%s' \"$PI_SESSION_ID|$PI_SESSION_FILE|$PI_PROVIDER|$PI_MODEL|$PI_REASONING_LEVEL|$PI_CODING_AGENT\""
        }),
    )
    .await;
    assert_eq!(
        content,
        json!([{
            "type": "text",
            "text": "session-file-harness|/sessions/current.jsonl|google|gemini-2.5-flash|high|true"
        }])
    );

    created
        .harness
        .set_model(pi_ai::providers::all::get_builtin_model("anthropic", "claude-sonnet-4-5").expect("model"))
        .await;
    created
        .harness
        .set_thinking_level(pi_agent_core::harness::types::ThinkingLevel::Low)
        .await;
    let bash = created
        .harness
        .get_tools()
        .await
        .into_iter()
        .find(|tool| tool.name() == "bash")
        .expect("bash tool");
    let content = execute_tool(
        bash,
        json!({
            "command": "printf '%s' \"${PI_SESSION_FILE+x}:$PI_SESSION_ID|$PI_PROVIDER|$PI_MODEL|$PI_REASONING_LEVEL|$PI_CODING_AGENT\""
        }),
    )
    .await;
    assert_eq!(
        content,
        json!([{
            "type": "text",
            "text": "x:session-file-harness|anthropic|claude-sonnet-4-5|low|true"
        }])
    );
}
