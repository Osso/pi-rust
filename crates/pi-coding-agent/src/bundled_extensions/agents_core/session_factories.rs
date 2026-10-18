//! Session factory contracts and prompt validation
//! (`extensions/agents-core/src/session-factories.ts`).
//!
//! Production child/attached session construction depends on unported
//! agent-session multi-agent identity options; see the traceability doc.

use pi_js_compat::string::{js_trim, utf16_len};
use serde::Serialize;

use super::shared::ChildAgentSessionFactory;

pub const MAX_GOAL_OBJECTIVE_CHARS: usize = 10_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GoalObjectiveValidation {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub objective: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub length: Option<usize>,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<GoalObjectiveFailure>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GoalObjectiveFailure {
    Empty,
    TooLong,
}

impl GoalObjectiveValidation {
    pub fn reason_str(&self) -> Option<&'static str> {
        self.reason.map(|reason| match reason {
            GoalObjectiveFailure::Empty => "empty",
            GoalObjectiveFailure::TooLong => "too_long",
        })
    }
}

pub fn validate_goal_objective(prompt: &str) -> GoalObjectiveValidation {
    let objective = js_trim(prompt);
    if objective.is_empty() {
        return GoalObjectiveValidation {
            objective: None,
            length: Some(0),
            ok: false,
            reason: Some(GoalObjectiveFailure::Empty),
        };
    }
    let length = utf16_len(objective);
    if length > MAX_GOAL_OBJECTIVE_CHARS {
        return GoalObjectiveValidation {
            objective: None,
            length: Some(length),
            ok: false,
            reason: Some(GoalObjectiveFailure::TooLong),
        };
    }
    GoalObjectiveValidation {
        objective: Some(objective.to_owned()),
        length: None,
        ok: true,
        reason: None,
    }
}

pub fn is_production_child_session_factory(factory: &ChildAgentSessionFactory) -> bool {
    factory.is_production()
}

pub fn spawn_prompt_validation_message(validation: &GoalObjectiveValidation) -> String {
    match validation.reason {
        Some(GoalObjectiveFailure::Empty) => "spawn_agent requires a non-empty prompt".to_owned(),
        Some(GoalObjectiveFailure::TooLong) | None => format!(
            "spawn_agent prompt too long ({} > {MAX_GOAL_OBJECTIVE_CHARS} chars)",
            validation
                .length
                .map_or_else(|| "undefined".to_owned(), |length| length.to_string())
        ),
    }
}

pub fn background_prompt_validation_message(validation: &GoalObjectiveValidation) -> String {
    match validation.reason {
        Some(GoalObjectiveFailure::Empty) => "Usage: /bg <prompt>".to_owned(),
        Some(GoalObjectiveFailure::TooLong) | None => format!(
            "Objective too long ({} > {MAX_GOAL_OBJECTIVE_CHARS} chars)",
            validation
                .length
                .map_or_else(|| "undefined".to_owned(), |length| length.to_string())
        ),
    }
}
