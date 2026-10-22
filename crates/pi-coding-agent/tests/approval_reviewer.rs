//! Behavioral tests for `supervisor/approval-reviewer.ts`.

use pi_js_compat::js_json::JsString;
use std::sync::Arc;

use pi_coding_agent::core::extensions::types::ToolCallEventResult;
use pi_coding_agent::core::session_control_db::SupervisorResponse;
use pi_coding_agent::supervisor::approval_reviewer::{
    SupervisorApprovalCall, SupervisorHumanReviewer, review_tool_call_with_supervisor,
};

fn decision(response: SupervisorResponse) -> SupervisorApprovalCall {
    Arc::new(move || {
        let response = response.clone();
        Box::pin(async move { Ok(response) })
    })
}

fn human(result: Option<ToolCallEventResult>) -> SupervisorHumanReviewer {
    Arc::new(move |reason: JsString| {
        let result = result.clone();
        Box::pin(async move {
            if let Some(mut result) = result {
                result.reason = Some(pi_js_compat::js_format!(
                    "{0}:{1}",
                    reason,
                    result.reason.unwrap_or_default()
                ));
                Ok(Some(result))
            } else {
                Ok(None)
            }
        })
    })
}

#[tokio::test]
async fn approves_without_a_block() {
    let result = review_tool_call_with_supervisor(
        decision(SupervisorResponse::Approve {
            reason: "ok".to_owned().into(),
        }),
        None,
        false,
    )
    .await
    .expect("decision");
    assert!(result.is_none());
}

#[tokio::test]
async fn rejects_with_the_supervisor_reason() {
    let result = review_tool_call_with_supervisor(
        decision(SupervisorResponse::Reject {
            reason: "not allowed".to_owned().into(),
        }),
        None,
        false,
    )
    .await
    .expect("decision")
    .expect("block");
    assert_eq!(result.block, Some(true));
    assert_eq!(result.reason.as_ref(), Some(&JsString::from("not allowed")));
}

#[tokio::test]
async fn escalates_rejection_to_the_human_reviewer() {
    let result = review_tool_call_with_supervisor(
        decision(SupervisorResponse::Reject {
            reason: "uncertain".to_owned().into(),
        }),
        Some(human(Some(ToolCallEventResult {
            block: Some(false),
            input: None,
            reason: Some("allowed".to_owned().into()),
            terminate: None,
        }))),
        true,
    )
    .await
    .expect("decision")
    .expect("human result");
    assert_eq!(result.block, Some(false));
    assert_eq!(result.reason.as_ref(), Some(&JsString::from("uncertain:allowed")));
}

#[tokio::test]
async fn errors_and_invalid_responses_fall_back_to_human_or_block() {
    let error = review_tool_call_with_supervisor(
        decision(SupervisorResponse::Error {
            reason: "supervisor down".to_owned().into(),
        }),
        None,
        false,
    )
    .await
    .expect("decision")
    .expect("block");
    assert_eq!(error.reason.as_ref(), Some(&JsString::from("supervisor down")));

    let invalid = review_tool_call_with_supervisor(
        decision(SupervisorResponse::Complete {
            reason: "done".to_owned().into(),
        }),
        None,
        false,
    )
    .await
    .expect("decision")
    .expect("block");
    assert_eq!(
        invalid.reason.as_ref(),
        Some(&JsString::from("Supervisor returned an invalid approval response"))
    );

    let escalated = review_tool_call_with_supervisor(
        decision(SupervisorResponse::Error {
            reason: "timeout".to_owned().into(),
        }),
        Some(human(None)),
        false,
    )
    .await
    .expect("decision");
    assert!(escalated.is_none());
}
