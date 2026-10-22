//! Port of `packages/coding-agent/src/supervisor/service.ts`.

use std::sync::Arc;
use std::time::Duration;

use pi_ai::request::BoxFuture;
use pi_js_compat::js_json::{JsObject, JsString, JsValue};
use tokio_util::sync::CancellationToken;

use crate::core::session_control_db::{
    ControlDbError, SupervisorRequest, SupervisorRequestKind, SupervisorResponse, complete_supervisor_request,
    has_pending_supervisor_approval_request, requeue_supervisor_request,
};
use pi_js_compat::js_json;

const DEFAULT_POLL_INTERVAL_MS: u64 = 50;

/// Evaluates one Supervisor prompt and returns the raw model response.
pub type SupervisorEvaluator =
    Arc<dyn Fn(String, CancellationToken) -> BoxFuture<'static, Result<JsValue, JsString>> + Send + Sync>;

/// Inputs for one Supervisor request evaluation.
#[derive(Clone)]
pub struct RunSupervisorRequestInput {
    pub control_db_path: String,
    pub evaluate: SupervisorEvaluator,
    pub request: SupervisorRequest,
    pub poll_interval_ms: Option<u64>,
}

fn goal_progress_response_contract() -> String {
    [
        "Use kind complete, pause, wait, continue, or error with a non-empty reason.",
        "Act as a peer unblocker, not a manager: preserve agent autonomy and intervene only on evidence-backed exceptions.",
        "Treat payload.objective as the authoritative full goal; compare all progress and completion claims against its complete scope, and a completed subtask must not replace broader scope.",
        "When the agent is making competent progress or can determine its own next step, use continue with instructions exactly \"Continue working toward the active goal.\"",
        "Use different continue instructions only when evidence identifies a concrete omission, such as unhandled pagination or a required omitted element; repeated failed or circular work; lost objective scope; or missing completion proof. Name only the exception and smallest corrective action.",
        "Do not restate the plan, prescribe routine steps, or invent oversight when evidence is uncertain.",
        "Use wait when progress is already underway asynchronously or depends on an external condition that can be rechecked, and no duplicate continuation should start.",
        "Use pause only when progress requires user action or input and cannot advance automatically.",
    ]
    .join("\n")
}

fn response_contract_for_request(kind: SupervisorRequestKind) -> String {
    match kind {
        SupervisorRequestKind::ApprovalReview => "Use kind approve or reject with a non-empty reason.".to_owned(),
        SupervisorRequestKind::GoalSetReview => [
            "Use kind set with a non-empty reason and objective.",
            "The returned objective must preserve every requirement and completion criterion in currentObjective, then add proposedObjective without narrowing existing scope.",
            "When currentObjective is absent, return proposedObjective unchanged.",
        ]
        .join("\n"),
        SupervisorRequestKind::SupervisorAdvisory => {
            "Use kind advisory with a non-empty answer. This response is advisory only and cannot direct or control the caller.".to_owned()
        }
        SupervisorRequestKind::GoalCompletionReview | SupervisorRequestKind::GoalIdleReview => {
            goal_progress_response_contract()
        }
    }
}

/// `buildSupervisorPrompt`.
pub fn build_supervisor_prompt(request: &SupervisorRequest) -> String {
    let response_contract = response_contract_for_request(request.kind);
    let payload = pi_js_compat::js_json!({
        "deadlineAt": request.deadline_at.clone(),
        "kind": request.kind,
        "payload": JsValue::Object(request.payload.clone()),
        "projectId": request.project_id.clone(),
        "senderSessionId": request.sender_session_id.clone(),
    });
    [
        "You are Pi Supervisor, a resident peer unblocker and policy engine.".to_owned(),
        "Evaluate only this bounded request, selectively reading Supervisor KB memory when necessary.".to_owned(),
        "Do not request or reconstruct historical session transcripts.".to_owned(),
        "You may read and write KB memory synchronously. Do not edit workspace files or control sessions, goals, processes, or agents.".to_owned(),
        format!("Project memory: memory/supervisor/{}.md", request.project_id),
        "Global memory: memory/supervisor/global.md".to_owned(),
        response_contract,
        "Call supervisor_response exactly once as the final action. Do not emit assistant text, JSON, markdown, or call end_turn before or after it.".to_owned(),
        "Request:".to_owned(),
        js_json::stringify_lossless_pretty(&payload),
    ]
    .join("\n")
}

fn response_text(response: &JsObject, key: &str) -> Option<JsString> {
    response.get(key)?.as_string().cloned()
}

/// `parseSupervisorResponse`. Content and arbitrary object keys stay UTF-16.
pub fn parse_supervisor_response(kind: SupervisorRequestKind, raw_response: &JsValue) -> Option<SupervisorResponse> {
    let response = parse_response_object(raw_response)?;
    let response_kind = response_text(&response, "kind")?.to_utf8().ok()?;
    if kind == SupervisorRequestKind::SupervisorAdvisory {
        let answer = response_text(&response, "answer")?;
        return (response_kind == "advisory" && !answer.trim().is_empty())
            .then_some(SupervisorResponse::Advisory { answer });
    }
    if kind == SupervisorRequestKind::GoalSetReview {
        let objective = response_text(&response, "objective")?.trim();
        let reason = response_text(&response, "reason")?;
        return (response_kind == "set" && !objective.is_empty())
            .then_some(SupervisorResponse::Set { objective, reason });
    }
    let reason = response_text(&response, "reason")?;
    match response_kind.as_str() {
        "error" => Some(SupervisorResponse::Error { reason }),
        name if kind == SupervisorRequestKind::ApprovalReview => match name {
            "approve" => Some(SupervisorResponse::Approve { reason }),
            "reject" => Some(SupervisorResponse::Reject { reason }),
            _ => None,
        },
        "complete" => Some(SupervisorResponse::Complete { reason }),
        "pause" => Some(SupervisorResponse::Pause { reason }),
        "wait" => Some(SupervisorResponse::Wait { reason }),
        "continue" => {
            let instructions = response_text(&response, "instructions")?;
            (!instructions.trim().is_empty()).then_some(SupervisorResponse::Continue { instructions, reason })
        }
        _ => None,
    }
}

fn parse_response_object(raw_response: &JsValue) -> Option<JsObject> {
    match raw_response {
        JsValue::String(text) => parse_json_object(text),
        JsValue::Object(response) => match extract_text_content(response) {
            Some(text) => parse_json_object(&text),
            None => Some(response.clone()),
        },
        _ => None,
    }
}

fn extract_text_content(response: &JsObject) -> Option<JsString> {
    let content = response.get("content")?.as_array()?;
    let text_part = content.iter().find(|item| {
        item.get("type")
            .and_then(JsValue::as_string)
            .is_some_and(|kind| kind == "text")
            && item.get("text").and_then(JsValue::as_string).is_some()
    })?;
    text_part.get("text")?.as_string().cloned()
}

fn parse_json_object(json_text: &JsString) -> Option<JsObject> {
    match js_json::parse_lossless(json_text.trim()) {
        Ok(JsValue::Object(object)) => Some(object),
        _ => None,
    }
}

/// Outcome of one request evaluation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunSupervisorRequestOutcome {
    Completed,
    Preempted,
}

/// `runSupervisorRequest`.
pub async fn run_supervisor_request(
    input: RunSupervisorRequestInput,
) -> Result<RunSupervisorRequestOutcome, ControlDbError> {
    let cancel = CancellationToken::new();
    let evaluation = (input.evaluate)(build_supervisor_prompt(&input.request), cancel.clone());
    let evaluation = tokio::spawn(evaluation);
    let poll_interval_ms = input.poll_interval_ms.unwrap_or(DEFAULT_POLL_INTERVAL_MS);
    let deadline_ms = pi_js_compat::date::date_parse_millis(&input.request.deadline_at);

    let raw_response = match wait_for_evaluation(&input, evaluation, &cancel, poll_interval_ms, deadline_ms).await? {
        EvaluationWait::Preempted => return Ok(RunSupervisorRequestOutcome::Preempted),
        EvaluationWait::Expired => return Ok(RunSupervisorRequestOutcome::Completed),
        EvaluationWait::Completed(Err(message)) => {
            complete_supervisor_request(
                &input.control_db_path,
                input.request.id,
                &required_claim_token(&input.request)?,
                &SupervisorResponse::Error { reason: message },
            )?;
            return Ok(RunSupervisorRequestOutcome::Completed);
        }
        EvaluationWait::Completed(Ok(value)) => value,
    };
    let response = parse_supervisor_response(input.request.kind, &raw_response).unwrap_or(SupervisorResponse::Error {
        reason: "Supervisor returned an invalid response".into(),
    });
    complete_supervisor_request(
        &input.control_db_path,
        input.request.id,
        &required_claim_token(&input.request)?,
        &response,
    )?;
    Ok(RunSupervisorRequestOutcome::Completed)
}

enum EvaluationWait {
    Completed(Result<JsValue, JsString>),
    Expired,
    Preempted,
}

async fn wait_for_evaluation(
    input: &RunSupervisorRequestInput,
    mut evaluation: tokio::task::JoinHandle<Result<JsValue, JsString>>,
    cancel: &CancellationToken,
    poll_interval_ms: u64,
    deadline_ms: Option<i64>,
) -> Result<EvaluationWait, ControlDbError> {
    loop {
        let remaining_ms = remaining_milliseconds(deadline_ms).max(1);
        let delay = Duration::from_millis(poll_interval_ms.min(remaining_ms));
        tokio::select! {
            result = &mut evaluation => {
                return Ok(EvaluationWait::Completed(
                    result.unwrap_or_else(|error| Err(error.to_string().into())),
                ));
            }
            () = tokio::time::sleep(delay) => {}
        }
        if now_millis() >= deadline_ms.unwrap_or(i64::MAX) {
            cancel.cancel();
            evaluation.abort();
            let _ = evaluation.await;
            complete_supervisor_request(
                &input.control_db_path,
                input.request.id,
                &required_claim_token(&input.request)?,
                &SupervisorResponse::Error {
                    reason: "Supervisor request deadline expired".to_owned().into(),
                },
            )?;
            return Ok(EvaluationWait::Expired);
        }
        if input.request.kind == SupervisorRequestKind::ApprovalReview
            || !has_pending_supervisor_approval_request(&input.control_db_path)?
        {
            continue;
        }
        cancel.cancel();
        evaluation.abort();
        let _ = evaluation.await;
        requeue_supervisor_request(
            &input.control_db_path,
            input.request.id,
            &required_claim_token(&input.request)?,
        )?;
        return Ok(EvaluationWait::Preempted);
    }
}

fn remaining_milliseconds(deadline_ms: Option<i64>) -> u64 {
    match deadline_ms {
        Some(deadline_ms) => (deadline_ms - now_millis()).max(1) as u64,
        None => DEFAULT_POLL_INTERVAL_MS,
    }
}

fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

fn required_claim_token(request: &SupervisorRequest) -> Result<String, ControlDbError> {
    request
        .claim_token
        .clone()
        .ok_or_else(|| ControlDbError::new(format!("Supervisor request {} has no claim token", request.id)))
}
