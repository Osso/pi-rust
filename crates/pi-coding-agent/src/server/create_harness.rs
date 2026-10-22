//! Port of `packages/coding-agent/src/server/create-harness.ts`.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use pi_agent_core::agent::types::{AgentTool, AgentToolResult, AgentToolUpdateCallback, ToolError};
use pi_agent_core::harness::agent_harness::{
    AgentHarness, AgentHarnessOptions, CreatedHarness, HarnessTool, SystemPromptSource,
};
use pi_agent_core::harness::tools::tool_context::ExecutionToolContext;
use pi_agent_core::harness::tools::{
    BashExecution, BashPrepare, BashTool, BashToolOptions, EditTool, ReadTool, WriteTool,
};
use pi_agent_core::harness::types::{AgentToolExecutionContext, ExecutionEnv, ToolExecutionMode};
use pi_ai::request::BoxFuture;
use pi_ai::types::ConstrainedSampling;
use serde_json::Value;
use tokio_util::sync::CancellationToken;

use crate::core::experimental::get_experimental_tool_sampling;
use crate::core::system_prompt::{BuildSystemPromptOptions, build_system_prompt};
use crate::core::tools::bash::BASH_TOOL_SYSTEM_PROMPT_CONTRIBUTION;
use crate::core::tools::edit::EDIT_TOOL_SYSTEM_PROMPT_CONTRIBUTION;
use crate::core::tools::read::READ_TOOL_SYSTEM_PROMPT_CONTRIBUTION;
use crate::core::tools::write::WRITE_TOOL_SYSTEM_PROMPT_CONTRIBUTION;

/// A harness tool plus its coding-agent prompt contribution.
#[derive(Clone)]
pub struct CodingAgentHarnessTool {
    pub tool: HarnessTool,
}

impl CodingAgentHarnessTool {
    pub fn new(tool: HarnessTool, prompt_snippet: Option<String>, prompt_guidelines: Vec<String>) -> Self {
        Self {
            tool: tool.with_prompt_metadata(prompt_snippet, prompt_guidelines),
        }
    }

    pub fn from_harness_tool(tool: HarnessTool) -> Self {
        Self { tool }
    }

    pub fn name(&self) -> &str {
        self.tool.name()
    }

    pub fn prompt_snippet(&self) -> Option<&str> {
        self.tool.prompt_snippet.as_deref()
    }

    pub fn prompt_guidelines(&self) -> &[String] {
        &self.tool.prompt_guidelines
    }
}

/// Options for [`build_coding_agent_harness_system_prompt`].
pub struct BuildCodingAgentHarnessSystemPromptOptions<'a> {
    pub cwd: &'a str,
    pub tools: &'a [CodingAgentHarnessTool],
    pub active_tool_names: &'a [String],
    pub system_prompt_options: Option<&'a BuildSystemPromptOptions>,
}

/// `buildCodingAgentHarnessSystemPrompt`.
pub fn build_coding_agent_harness_system_prompt(options: BuildCodingAgentHarnessSystemPromptOptions<'_>) -> String {
    let active_tools: Vec<&CodingAgentHarnessTool> = options
        .active_tool_names
        .iter()
        .filter_map(|name| options.tools.iter().find(|tool| tool.name() == name))
        .collect();
    let mut tool_snippets: HashMap<String, String> = HashMap::new();
    for tool in &active_tools {
        let Some(prompt_snippet) = tool.prompt_snippet() else {
            continue;
        };
        let collapsed = prompt_snippet
            .replace(['\r', '\n'], " ")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        if !collapsed.is_empty() {
            tool_snippets.insert(tool.name().to_owned(), collapsed);
        }
    }
    let prompt_guidelines: Vec<String> = active_tools
        .iter()
        .flat_map(|tool| tool.prompt_guidelines().iter().cloned())
        .collect();
    let base = options.system_prompt_options.cloned().unwrap_or_default();
    build_system_prompt(&BuildSystemPromptOptions {
        cwd: options.cwd.to_owned(),
        selected_tools: Some(active_tools.iter().map(|tool| tool.name().to_owned()).collect()),
        tool_snippets: Some(tool_snippets),
        prompt_guidelines: Some(prompt_guidelines),
        ..base
    })
}

/// Options accepted by [`create_coding_agent_harness`].
pub struct CreateCodingAgentHarnessOptions {
    /// Harness options; `tools` and `system_prompt` act as the caller-supplied overrides.
    pub harness: AgentHarnessOptions,
    pub env: Arc<dyn ExecutionEnv>,
    pub bash_command_prefix: Option<String>,
    /// Path to the JSONL session file exposed to default bash commands as `PI_SESSION_FILE`.
    pub session_file: Option<String>,
    pub tools: Option<Vec<CodingAgentHarnessTool>>,
    pub system_prompt_options: Option<BuildSystemPromptOptions>,
}

/// Result of [`create_coding_agent_harness`].
pub struct CreatedCodingAgentHarness {
    pub harness: Arc<AgentHarness>,
    pub suspended: Vec<pi_agent_core::harness::agent_harness::SuspendedOperation>,
}

/// Tool wrapper that adds the experimental constrained-sampling configuration.
struct ConstrainedSamplingTool {
    inner: Arc<dyn AgentTool>,
    constrained_sampling: Option<ConstrainedSampling>,
}

impl AgentTool for ConstrainedSamplingTool {
    fn name(&self) -> &str {
        self.inner.name()
    }

    fn description(&self) -> &str {
        self.inner.description()
    }

    fn parameters(&self) -> &Value {
        self.inner.parameters()
    }

    fn label(&self) -> &str {
        self.inner.label()
    }

    fn constrained_sampling(&self) -> Option<ConstrainedSampling> {
        self.constrained_sampling.clone()
    }

    fn execution_mode(&self) -> Option<ToolExecutionMode> {
        self.inner.execution_mode()
    }

    fn prepare_arguments(
        &self,
        args: pi_js_compat::js_json::JsValue,
    ) -> Result<pi_js_compat::js_json::JsValue, pi_agent_core::agent::ToolError> {
        self.inner.prepare_arguments(args)
    }

    fn prepare_execution(
        &self,
        tool_call_id: String,
        signal: Option<CancellationToken>,
    ) -> BoxFuture<'static, Result<Option<pi_ai::types::ToolCallExecution>, pi_agent_core::agent::ToolError>> {
        self.inner.prepare_execution(tool_call_id, signal)
    }

    fn execute(
        &self,
        tool_call_id: String,
        params: pi_js_compat::js_json::JsValue,
        signal: Option<CancellationToken>,
        on_update: AgentToolUpdateCallback,
        execution: Option<AgentToolExecutionContext>,
    ) -> BoxFuture<'static, Result<AgentToolResult, ToolError>> {
        self.inner.execute(tool_call_id, params, signal, on_update, execution)
    }
}

fn coding_agent_tool(
    tool: Arc<dyn AgentTool>,
    prompt_snippet: &str,
    prompt_guidelines: &[&str],
) -> CodingAgentHarnessTool {
    let constrained_sampling = get_experimental_tool_sampling().map(ConstrainedSampling::Config);
    CodingAgentHarnessTool::new(
        HarnessTool::new(Arc::new(ConstrainedSamplingTool {
            inner: tool,
            constrained_sampling,
        })),
        Some(prompt_snippet.to_owned()),
        prompt_guidelines
            .iter()
            .map(|guideline| (*guideline).to_owned())
            .collect(),
    )
}

/// Bash hook that exports the live harness model state to the shell.
struct HarnessBashPrepare {
    harness: Arc<Mutex<Option<Arc<AgentHarness>>>>,
    metadata_id: String,
    session_file: String,
}

impl BashPrepare for HarnessBashPrepare {
    fn prepare<'a>(
        &'a self,
        execution: &'a mut BashExecution,
        _context: &'a ExecutionToolContext,
        _signal: Option<&'a CancellationToken>,
    ) -> BoxFuture<'a, Result<(), String>> {
        Box::pin(async move {
            let harness = self
                .harness
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .clone()
                .ok_or_else(|| "Coding-agent Harness callback ran before Harness initialization".to_owned())?;
            let model = harness.get_model().await;
            let thinking_level = harness.get_thinking_level().await;
            execution
                .env
                .insert("PI_SESSION_ID".to_owned(), self.metadata_id.clone());
            execution
                .env
                .insert("PI_SESSION_FILE".to_owned(), self.session_file.clone());
            execution.env.insert("PI_PROVIDER".to_owned(), model.provider);
            execution.env.insert("PI_MODEL".to_owned(), model.id);
            execution
                .env
                .insert("PI_REASONING_LEVEL".to_owned(), thinking_level.as_str().to_owned());
            Ok(())
        })
    }
}

fn default_tools(
    harness: Arc<Mutex<Option<Arc<AgentHarness>>>>,
    metadata_id: String,
    env: Arc<dyn ExecutionEnv>,
    bash_command_prefix: Option<String>,
    session_file: Option<String>,
) -> Vec<CodingAgentHarnessTool> {
    let context = Arc::new(ExecutionToolContext::new(env));
    vec![
        coding_agent_tool(
            Arc::new(ReadTool::new(Arc::clone(&context))),
            READ_TOOL_SYSTEM_PROMPT_CONTRIBUTION.snippet,
            READ_TOOL_SYSTEM_PROMPT_CONTRIBUTION.guidelines,
        ),
        coding_agent_tool(
            Arc::new(BashTool::with_options(
                Arc::clone(&context),
                BashToolOptions {
                    command_prefix: bash_command_prefix,
                    prepare: Some(Arc::new(HarnessBashPrepare {
                        harness,
                        metadata_id,
                        session_file: session_file.unwrap_or_default(),
                    })),
                },
            )),
            BASH_TOOL_SYSTEM_PROMPT_CONTRIBUTION.snippet,
            BASH_TOOL_SYSTEM_PROMPT_CONTRIBUTION.guidelines,
        ),
        coding_agent_tool(
            Arc::new(EditTool::new(Arc::clone(&context))),
            EDIT_TOOL_SYSTEM_PROMPT_CONTRIBUTION.snippet,
            EDIT_TOOL_SYSTEM_PROMPT_CONTRIBUTION.guidelines,
        ),
        coding_agent_tool(
            Arc::new(WriteTool::new(Arc::clone(&context))),
            WRITE_TOOL_SYSTEM_PROMPT_CONTRIBUTION.snippet,
            WRITE_TOOL_SYSTEM_PROMPT_CONTRIBUTION.guidelines,
        ),
    ]
}

fn dynamic_system_prompt(
    harness: Arc<Mutex<Option<Arc<AgentHarness>>>>,
    cwd: String,
    system_prompt_options: Option<BuildSystemPromptOptions>,
) -> SystemPromptSource {
    SystemPromptSource::Provider(Arc::new(move || {
        let harness = harness.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone();
        let cwd = cwd.clone();
        let system_prompt_options = system_prompt_options.clone();
        Box::pin(async move {
            let harness = harness.expect("Coding-agent Harness callback ran before Harness initialization");
            let current_tools: Vec<CodingAgentHarnessTool> = harness
                .get_tools()
                .await
                .into_iter()
                .map(CodingAgentHarnessTool::from_harness_tool)
                .collect();
            let active_tool_names = harness.get_active_tools().await;
            build_coding_agent_harness_system_prompt(BuildCodingAgentHarnessSystemPromptOptions {
                cwd: &cwd,
                tools: &current_tools,
                active_tool_names: &active_tool_names,
                system_prompt_options: system_prompt_options.as_ref(),
            })
        })
    }))
}

/// `createCodingAgentHarness`.
pub async fn create_coding_agent_harness(
    options: CreateCodingAgentHarnessOptions,
) -> Result<CreatedCodingAgentHarness, String> {
    let CreateCodingAgentHarnessOptions {
        mut harness,
        env,
        bash_command_prefix,
        session_file,
        tools: provided_tools,
        system_prompt_options,
    } = options;
    let harness_slot: Arc<Mutex<Option<Arc<AgentHarness>>>> = Arc::new(Mutex::new(None));
    let cwd = env.cwd().to_owned();
    let tools = match provided_tools {
        Some(tools) => tools,
        None => {
            let metadata = harness
                .session
                .get_metadata()
                .await
                .map_err(|error| error.to_string())?;
            default_tools(
                Arc::clone(&harness_slot),
                metadata.id,
                Arc::clone(&env),
                bash_command_prefix,
                session_file,
            )
        }
    };
    let active_tool_names = harness
        .active_tool_names
        .take()
        .unwrap_or_else(|| tools.iter().map(|tool| tool.name().to_owned()).collect());
    let system_prompt = harness
        .system_prompt
        .take()
        .unwrap_or_else(|| dynamic_system_prompt(Arc::clone(&harness_slot), cwd, system_prompt_options));
    harness.tools = Some(tools.iter().map(|tool| tool.tool.clone()).collect());
    harness.active_tool_names = Some(active_tool_names);
    harness.system_prompt = Some(system_prompt);
    let CreatedHarness { harness, suspended } =
        AgentHarness::create(harness).await.map_err(|error| error.to_string())?;
    let harness = Arc::new(harness);
    *harness_slot.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(Arc::clone(&harness));
    Ok(CreatedCodingAgentHarness { harness, suspended })
}
