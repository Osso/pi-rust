//! Port of `packages/coding-agent/src/supervisor/approval-reviewer.ts`.

use std::sync::Arc;

use pi_ai::request::BoxFuture;

use crate::core::extensions::types::ToolCallEventResult;
use crate::core::session_control_db::SupervisorResponse;

/// Requests a supervisor decision.
pub type SupervisorApprovalCall = Arc<dyn Fn() -> BoxFuture<'static, Result<SupervisorResponse, String>> + Send + Sync>;

/// Escalates an uncertain decision to a human reviewer. A failure propagates
/// like the source's rejected `askHuman` promise.
pub type SupervisorHumanReviewer =
    Arc<dyn Fn(String) -> BoxFuture<'static, Result<Option<ToolCallEventResult>, String>> + Send + Sync>;

/// `reviewToolCallWithSupervisor`.
pub async fn review_tool_call_with_supervisor(
    request_decision: SupervisorApprovalCall,
    ask_human: Option<SupervisorHumanReviewer>,
    escalate_rejection: bool,
) -> Result<Option<ToolCallEventResult>, String> {
    let decision = request_decision().await?;
    match decision {
        SupervisorResponse::Approve { .. } => Ok(None),
        SupervisorResponse::Reject { reason } => match (escalate_rejection, ask_human) {
            (true, Some(ask_human)) => ask_human(reason).await,
            _ => Ok(Some(block(reason))),
        },
        SupervisorResponse::Error { reason } => match ask_human {
            Some(ask_human) => ask_human(reason).await,
            None => Ok(Some(block(reason))),
        },
        _ => {
            let reason = "Supervisor returned an invalid approval response".to_owned();
            match ask_human {
                Some(ask_human) => ask_human(reason).await,
                None => Ok(Some(block(reason))),
            }
        }
    }
}

fn block(reason: String) -> ToolCallEventResult {
    ToolCallEventResult {
        block: Some(true),
        input: None,
        reason: Some(reason),
        terminate: None,
    }
}
