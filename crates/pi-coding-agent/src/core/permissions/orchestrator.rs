//! Port of `packages/coding-agent/src/core/permissions/orchestrator.ts`.

use std::sync::Arc;

use pi_ai::request::BoxFuture;

use super::policy::{ApprovalPolicy, ApprovalPolicyContext, ApprovalPolicyDecision, evaluate_approval_policy};
use crate::core::extensions::types::ToolCallEventResult;

/// Async approval reviewer.
pub type ApprovalReviewer = Arc<dyn Fn() -> BoxFuture<'static, Option<ToolCallEventResult>> + Send + Sync>;

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
pub async fn orchestrate_tool_approval(options: &ApprovalOrchestratorOptions) -> Option<ToolCallEventResult> {
    let decision = evaluate_approval_policy(
        options.policy,
        ApprovalPolicyContext {
            approval_required: options.approval_required,
        },
    );

    match decision {
        ApprovalPolicyDecision::Allow => None,
        ApprovalPolicyDecision::Block { reason } => Some(ToolCallEventResult {
            block: Some(true),
            reason: Some(reason),
            terminate: None,
        }),
        ApprovalPolicyDecision::Review => {
            if let Some(hook_reviewer) = &options.hook_reviewer
                && let Some(decision) = hook_reviewer().await
            {
                return Some(decision);
            }
            if let Some(llm_reviewer) = &options.llm_reviewer {
                return llm_reviewer().await;
            }
            match &options.reviewer {
                Some(reviewer) => reviewer().await,
                None => None,
            }
        }
    }
}
