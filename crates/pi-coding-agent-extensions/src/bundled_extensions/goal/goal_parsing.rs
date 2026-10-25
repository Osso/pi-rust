//! Port of `packages/coding-agent/extensions/goal/src/goal-parsing.ts`.

use pi_js_compat::js_json::{self, JsString, JsValue as Value};

use super::goal_types::{Goal, GoalReviewEvidenceEvent};

pub fn is_record(value: &Value) -> bool {
    value.is_object()
}

pub fn optional_string(value: Option<&Value>) -> Option<JsString> {
    value.and_then(Value::as_string).cloned()
}

fn optional_number(value: Option<&Value>) -> Option<f64> {
    value.and_then(Value::as_f64).filter(|number| number.is_finite())
}

fn parse_review_evidence_event(value: &Value) -> Option<GoalReviewEvidenceEvent> {
    if !is_record(value) {
        return None;
    }
    match value.get("kind").and_then(Value::as_string) {
        Some(kind) if kind == "user" => Some(GoalReviewEvidenceEvent::User {
            text: value.get("text")?.as_string()?.to_owned(),
        }),
        Some(kind) if kind == "end_turn" => {
            let reason = value.get("reason")?.as_string()?;
            if reason.trim().is_empty() {
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
        .map(|objective| objective.trim())
        .filter(|objective| !objective.is_empty())?;
    let branch = optional_string(value.get("branch")).filter(|branch| !branch.is_empty())?;
    let created_at = optional_string(value.get("createdAt")).filter(|created_at| !created_at.is_empty())?;
    let pause_reason = optional_string(value.get("pauseReason"))
        .map(|reason| reason.trim())
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
pub fn parse_goal_json(value: &JsString) -> Option<Goal> {
    parse_goal(&js_json::parse_lossless(value).ok()?)
}

#[cfg(test)]
mod lossless_tests {
    use super::*;

    fn parse(text: &str) -> Option<Goal> {
        parse_goal_json(&JsString::from(text))
    }
    #[test]
    fn goal_json_preserves_objective_and_evidence_units() {
        let goal = parse(r#"{"objective":"x\ud800","branch":"main","createdAt":"now","reviewEvidence":[{"kind":"user","text":"y\udc00"}]}"#).unwrap();
        assert_eq!(goal.objective.units(), &[120, 0xd800]);
        let json = serde_json::to_string(&goal).unwrap();
        let restored = parse(&json).unwrap();
        assert_eq!(restored, goal);
    }

    #[test]
    fn goal_json_preserves_identity_and_timestamp_units() {
        let goal = parse(
            r#"{"objective":"o","branch":"b\ud800","createdAt":"c\udc00","completedAt":"d\ud801","pausedAt":"p\udfff"}"#,
        )
        .expect("lone surrogates are ordinary JavaScript strings");
        assert_eq!(goal.branch.units(), &[98, 0xd800]);
        assert_eq!(goal.created_at.units(), &[99, 0xdc00]);
        assert_eq!(
            goal.completed_at.as_ref().map(JsString::units),
            Some(&[100, 0xd801][..])
        );
        assert_eq!(goal.paused_at.as_ref().map(JsString::units), Some(&[112, 0xdfff][..]));
    }
}
