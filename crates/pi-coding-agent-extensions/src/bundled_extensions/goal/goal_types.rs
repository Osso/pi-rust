//! Port of `packages/coding-agent/extensions/goal/src/goal-types.ts`.

use std::sync::Arc;

use pi_js_compat::js_json::{JsObject, JsString};
use pi_js_compat::number as js_number;
use serde::{Deserialize, Serialize};

use crate::core::extensions::types::{BoxFuture, ExtensionContext, HandlerError};

/// A conversation event retained for the next Supervisor goal review.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum GoalReviewEvidenceEvent {
    User { text: JsString },
    EndTurn { reason: JsString },
}

/// Durable goal state.
///
/// Field order matches the TypeScript object literal because the goal is stored
/// as JSON in the session metadata row.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Goal {
    pub objective: JsString,
    pub branch: JsString,
    pub created_at: JsString,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<JsString>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completion_reason: Option<JsString>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "js_number::serialize_option"
    )]
    pub continuation_turns: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub paused_at: Option<JsString>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pause_reason: Option<JsString>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub review_evidence: Option<Vec<GoalReviewEvidenceEvent>>,
}

/// The frozen goal decision union, including its optional grouped display ID.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum GoalSupervisorResponse {
    Complete {
        reason: JsString,
        #[serde(default, rename = "displayId", skip_serializing_if = "Option::is_none")]
        display_id: Option<JsString>,
    },
    Continue {
        instructions: JsString,
        reason: JsString,
        #[serde(default, rename = "displayId", skip_serializing_if = "Option::is_none")]
        display_id: Option<JsString>,
    },
    Pause {
        reason: JsString,
        #[serde(default, rename = "displayId", skip_serializing_if = "Option::is_none")]
        display_id: Option<JsString>,
    },
    Wait {
        reason: JsString,
        #[serde(default, rename = "displayId", skip_serializing_if = "Option::is_none")]
        display_id: Option<JsString>,
    },
    Set {
        objective: JsString,
        reason: JsString,
        #[serde(default, rename = "displayId", skip_serializing_if = "Option::is_none")]
        display_id: Option<JsString>,
    },
    Error {
        reason: JsString,
        #[serde(default, rename = "displayId", skip_serializing_if = "Option::is_none")]
        display_id: Option<JsString>,
    },
}

impl GoalSupervisorResponse {
    pub fn display_id(&self) -> Option<&JsString> {
        match self {
            Self::Complete { display_id, .. }
            | Self::Continue { display_id, .. }
            | Self::Pause { display_id, .. }
            | Self::Wait { display_id, .. }
            | Self::Set { display_id, .. }
            | Self::Error { display_id, .. } => display_id.as_ref(),
        }
    }

    pub fn set_display_id(&mut self, id: JsString) {
        match self {
            Self::Complete { display_id, .. }
            | Self::Continue { display_id, .. }
            | Self::Pause { display_id, .. }
            | Self::Wait { display_id, .. }
            | Self::Set { display_id, .. }
            | Self::Error { display_id, .. } => *display_id = Some(id),
        }
    }
}

/// Input passed to a goal review.
#[derive(Clone)]
pub struct GoalReviewInput {
    pub kind: GoalReviewKind,
    pub payload: JsObject,
    pub ctx: ExtensionContext,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GoalReviewKind {
    GoalCompletionReview,
    GoalIdleReview,
    GoalSetReview,
}

impl GoalReviewKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::GoalCompletionReview => "goal_completion_review",
            Self::GoalIdleReview => "goal_idle_review",
            Self::GoalSetReview => "goal_set_review",
        }
    }
}

pub type GoalSupervisorReview =
    Arc<dyn Fn(GoalReviewInput) -> BoxFuture<'static, Result<GoalSupervisorResponse, HandlerError>> + Send + Sync>;

#[derive(Debug, Clone, PartialEq)]
pub struct ReviewedGoalResponse {
    pub decision: GoalSupervisorResponse,
    pub evidence_count: usize,
}

pub type GoalEvidenceReview =
    Arc<dyn Fn(GoalReviewInput) -> BoxFuture<'static, Result<ReviewedGoalResponse, HandlerError>> + Send + Sync>;

/// Extension options; `review_goal` overrides the resident Supervisor client.
#[derive(Clone, Default)]
pub struct GoalExtensionOptions {
    pub review_goal: Option<GoalSupervisorReview>,
}
