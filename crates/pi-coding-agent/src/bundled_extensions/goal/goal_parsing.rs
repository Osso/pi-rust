//! Port of `packages/coding-agent/extensions/goal/src/goal-parsing.ts`.

use pi_js_compat::string::js_trim;
use serde_json::Value;

use super::goal_types::{Goal, GoalReviewEvidenceEvent};

pub fn is_record(value: &Value) -> bool {
    value.is_object()
}

pub fn optional_string(value: Option<&Value>) -> Option<String> {
    value.and_then(Value::as_str).map(str::to_owned)
}

fn optional_number(value: Option<&Value>) -> Option<f64> {
    value.and_then(Value::as_f64).filter(|number| number.is_finite())
}

fn parse_review_evidence_event(value: &Value) -> Option<GoalReviewEvidenceEvent> {
    if !is_record(value) {
        return None;
    }
    match value.get("kind").and_then(Value::as_str) {
        Some("user") => Some(GoalReviewEvidenceEvent::User {
            text: value.get("text")?.as_str()?.to_owned(),
        }),
        Some("end_turn") => {
            let reason = value.get("reason")?.as_str()?;
            if js_trim(reason).is_empty() {
                return None;
            }
            Some(GoalReviewEvidenceEvent::EndTurn {
                reason: reason.to_owned(),
            })
        }
        _ => None,
    }
}

fn parse_review_evidence(value: Option<&Value>) -> Option<Vec<GoalReviewEvidenceEvent>> {
    let events = value?.as_array()?;
    let evidence: Vec<_> = events.iter().filter_map(parse_review_evidence_event).collect();
    if evidence.is_empty() {
        return None;
    }
    Some(evidence)
}

/// `parseGoal`.
pub fn parse_goal(value: &Value) -> Option<Goal> {
    if !is_record(value) {
        return None;
    }
    let objective = optional_string(value.get("objective"))
        .map(|objective| js_trim(&objective).to_owned())
        .filter(|objective| !objective.is_empty())?;
    let branch = optional_string(value.get("branch")).filter(|branch| !branch.is_empty())?;
    let created_at = optional_string(value.get("createdAt")).filter(|created_at| !created_at.is_empty())?;
    let pause_reason = optional_string(value.get("pauseReason"))
        .map(|reason| js_trim(&reason).to_owned())
        .filter(|reason| !reason.is_empty());
    Some(Goal {
        objective,
        branch,
        created_at,
        completed_at: optional_string(value.get("completedAt")),
        completion_reason: optional_string(value.get("completionReason")),
        continuation_turns: optional_number(value.get("continuationTurns")),
        paused_at: optional_string(value.get("pausedAt")),
        pause_reason,
        review_evidence: parse_review_evidence(value.get("reviewEvidence")),
    })
}

/// `parseGoalJson`.
pub fn parse_goal_json(value: &str) -> Option<Goal> {
    parse_goal(&serde_json::from_str(value).ok()?)
}
