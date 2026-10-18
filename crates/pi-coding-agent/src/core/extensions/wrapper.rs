//! Tool wrappers for extension-registered tools (`core/extensions/wrapper.ts`).
//!
//! Tool-call and tool-result interception lives in the agent session; these
//! wrappers only adapt execution so extension tools receive the runner context.

use std::sync::Arc;

use pi_agent_core::AgentToolExecutionContext;
use pi_agent_core::agent::types::{AgentTool, AgentToolResult, AgentToolUpdateCallback, ToolError};
use pi_agent_core::harness::types::ToolExecutionMode;
use pi_ai::types::{ConstrainedSampling, ToolCallExecution};
use serde_json::Value;
use tokio_util::sync::CancellationToken;

use super::runner::ExtensionRunner;
use super::types::{BoxFuture, ExtensionContext, HandlerError, RegisteredTool, ToolDefinition};

struct WrappedExtensionTool {
    definition: ToolDefinition,
    runner: ExtensionRunner,
}

impl AgentTool for WrappedExtensionTool {
    fn name(&self) -> &str {
        &self.definition.name
    }

    fn description(&self) -> &str {
        &self.definition.description
    }

    fn parameters(&self) -> &Value {
        &self.definition.parameters
    }

    fn label(&self) -> &str {
        &self.definition.label
    }

    fn constrained_sampling(&self) -> Option<ConstrainedSampling> {
        self.definition.constrained_sampling.clone()
    }

    fn execution_mode(&self) -> Option<ToolExecutionMode> {
        self.definition.execution_mode
    }

    fn prepare_execution(
        &self,
        tool_call_id: String,
        signal: Option<CancellationToken>,
    ) -> BoxFuture<'static, Option<ToolCallExecution>> {
        let Some(prepare) = self.definition.prepare_execution.clone() else {
            return Box::pin(async { None });
        };
        let ctx = self.runner.create_context();
        Box::pin(async move { prepare(tool_call_id, signal, ctx).await.ok().flatten() })
    }

    fn execute(
        &self,
        tool_call_id: String,
        params: Value,
        signal: Option<CancellationToken>,
        on_update: AgentToolUpdateCallback,
        execution: Option<AgentToolExecutionContext>,
    ) -> BoxFuture<'static, Result<AgentToolResult, ToolError>> {
        let definition = self.definition.clone();
        let runner = self.runner.clone();
        Box::pin(async move {
            let mut context = runner.create_context();
            if let Some(execution) = execution {
                context.tool_execution_started_at = Some(execution.started_at);
                context.tool_execution_agent_id = execution.agent_id;
            }

            let active_before = runner
                .get_active_tools()
                .map_err(|error| ToolError::new(error.message))?;
            let result = (definition.execute)(tool_call_id, params, signal, Some(on_update), context)
                .await
                .map_err(|error| ToolError::new(error.message))?;
            let active_after = runner
                .get_active_tools()
                .map_err(|error| ToolError::new(error.message))?;
            let active_after_names: Vec<&String> = active_after.iter().collect();
            if !active_before.iter().all(|name| active_after_names.contains(&name)) {
                return Ok(result);
            }

            let added_tool_names: Vec<String> = active_after
                .into_iter()
                .filter(|name| !active_before.contains(name))
                .collect();
            if added_tool_names.is_empty() {
                return Ok(result);
            }

            let mut combined = result.added_tool_names.unwrap_or_default();
            for name in added_tool_names {
                if !combined.contains(&name) {
                    combined.push(name);
                }
            }
            Ok(AgentToolResult {
                added_tool_names: Some(combined),
                ..result
            })
        })
    }
}

/// Wrap a registered extension tool into an `AgentTool`.
pub fn wrap_registered_tool(registered_tool: RegisteredTool, runner: &ExtensionRunner) -> Arc<dyn AgentTool> {
    Arc::new(WrappedExtensionTool {
        definition: registered_tool.definition,
        runner: runner.clone(),
    })
}

/// Wrap all registered extension tools into `AgentTool`s.
pub fn wrap_registered_tools(
    registered_tools: Vec<RegisteredTool>,
    runner: &ExtensionRunner,
) -> Vec<Arc<dyn AgentTool>> {
    registered_tools
        .into_iter()
        .map(|tool| wrap_registered_tool(tool, runner))
        .collect()
}

/// Adapt an `AgentHarnessTool<ExtensionContext>` into an extension `ToolDefinition`.
///
/// First-party coordination tools implement the harness trait; the definition
/// shape is what `createAllToolDefinitions` exposes to renderers and extensions.
pub fn harness_tool_definition<T>(tool: Arc<T>) -> ToolDefinition
where
    T: pi_agent_core::harness::types::AgentHarnessTool<ExtensionContext> + 'static,
{
    let execute_tool = Arc::clone(&tool);
    let arguments_tool = Arc::clone(&tool);
    let prepare_tool = Arc::clone(&tool);
    ToolDefinition {
        name: tool.name().to_owned(),
        label: tool.label().to_owned(),
        description: tool.description().to_owned(),
        prompt_snippet: None,
        prompt_guidelines: None,
        approval_required: None,
        approval_kind: None,
        parameters: tool.parameters().clone(),
        constrained_sampling: None,
        render_shell: None,
        prepare_arguments: Some(Arc::new(move |args: &Value| {
            Ok(arguments_tool.prepare_arguments(args.clone()))
        })),
        prepare_execution: Some(Arc::new(move |tool_call_id, signal, _context| {
            let prepare_tool = Arc::clone(&prepare_tool);
            Box::pin(async move { Ok(prepare_tool.prepare_execution(tool_call_id, signal).await) })
        })),
        execution_mode: tool.execution_mode(),
        execute: Arc::new(move |tool_call_id, params, signal, on_update, context| {
            let tool = Arc::clone(&execute_tool);
            Box::pin(async move {
                tool.execute(tool_call_id, params, signal, on_update, context, None)
                    .await
                    .map_err(|error| HandlerError::new(error.message))
            })
        }),
        render_call: None,
        render_result: None,
    }
}
