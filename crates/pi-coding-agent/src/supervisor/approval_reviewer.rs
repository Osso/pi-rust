//! Port of `packages/coding-agent/src/supervisor/approval-reviewer.ts`.

use std::sync::Arc;

use pi_agent_core::harness::types::BeforeToolCallResult;
use pi_ai::request::BoxFuture;

use crate::core::session_control_db::SupervisorResponse;

/// Requests one durable Supervisor decision.
pub type SupervisorApprovalCall = Arc<dyn Fn() -> BoxFuture<'static, SupervisorResponse> + Send + Sync>;
/// Escalates a blocked decision to the human reviewer.
pub type SupervisorHumanReviewer =
    Arc<dyn Fn(String) -> BoxFuture<'static, Option<BeforeToolCallResult>> + Send + Sync>;

/// `reviewToolCallWithSupervisor`.
pub async fn review_tool_call_with_supervisor(
    request_decision: SupervisorApprovalCall,
    ask_human: Option<SupervisorHumanReviewer>,
    escalate_rejection: bool,
) -> Option<BeforeToolCallResult> {
    let decision = request_decision().await;
    match decision {
        SupervisorResponse::Approve { .. } => None,
        SupervisorResponse::Reject { reason } => {
            if escalate_rejection && let Some(ask_human) = ask_human {
                return ask_human(reason).await;
            }
            Some(block(reason))
        }
        SupervisorResponse::Error { reason } => match ask_human {
            Some(ask_human) => ask_human(reason).await,
            None => Some(block(reason)),
        },
        _ => {
            let reason = "Supervisor returned an invalid approval response".to_owned();
            match ask_human {
                Some(ask_human) => ask_human(reason).await,
                None => Some(block(reason)),
            }
        }
    }
}

fn block(reason: String) -> BeforeToolCallResult {
    BeforeToolCallResult {
        block: Some(true),
        reason: Some(reason),
        terminate: None,
    }
}
