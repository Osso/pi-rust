//! Port of `packages/coding-agent/src/core/permissions/orchestrator.ts`.

use std::sync::Arc;

use pi_ai::request::BoxFuture;

use super::policy::{ApprovalPolicy, ApprovalPolicyContext, ApprovalPolicyDecision, evaluate_approval_policy};
use crate::core::extensions::types::{HandlerError, ToolCallEventResult};

/// Async approval reviewer. A failure propagates like the source's thrown
/// reviewer promise and aborts the tool call.
pub type ApprovalReviewer =
    Arc<dyn Fn() -> BoxFuture<'static, Result<Option<ToolCallEventResult>, HandlerError>> + Send + Sync>;

/// Inputs for [`orchestrate_tool_approval`].
#[derive(Clone)]
pub struct ApprovalOrchestratorOptions {
    pub policy: ApprovalPolicy,
    pub approval_required: bool,
    pub reviewer: Option<ApprovalReviewer>,
    pub hook_reviewer: Option<ApprovalReviewer>,
    pub llm_reviewer: Option<ApprovalReviewer>,
}

/// Run the approval policy, then the hook, LLM and generic reviewers in order.
pub async fn orchestrate_tool_approval(
    options: &ApprovalOrchestratorOptions,
) -> Result<Option<ToolCallEventResult>, HandlerError> {
    let decision = evaluate_approval_policy(
        options.policy,
        ApprovalPolicyContext {
            approval_required: options.approval_required,
        },
    );

    match decision {
        ApprovalPolicyDecision::Allow => Ok(None),
        ApprovalPolicyDecision::Block { reason } => Ok(Some(ToolCallEventResult {
            block: Some(true),
            input: None,
            reason: Some(reason),
            terminate: None,
        })),
        ApprovalPolicyDecision::Review => {
            if let Some(hook_reviewer) = &options.hook_reviewer
                && let Some(decision) = hook_reviewer().await?
            {
                return Ok(Some(decision));
            }
            if let Some(llm_reviewer) = &options.llm_reviewer {
                return llm_reviewer().await;
            }
            match &options.reviewer {
                Some(reviewer) => reviewer().await,
                None => Ok(None),
            }
        }
    }
}
