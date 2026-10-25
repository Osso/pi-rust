//! Port of `packages/coding-agent/extensions/goal/src/goal-agent-end.ts`.

use std::sync::Arc;

use crate::core::extensions::types::{AgentEndEvent, BoxFuture, ExtensionContext, HandlerError};

use super::goal_scheduling::TerminalTurn;
use super::goal_types::{Goal, GoalEvidenceReview, GoalReviewInput, GoalReviewKind, ReviewedGoalResponse};

pub type IsSameGoal = Arc<dyn Fn(ExtensionContext, &Goal) -> Result<bool, HandlerError> + Send + Sync>;

pub type ApplyDecision = Arc<
    dyn Fn(ReviewedGoalResponse, Goal, ExtensionContext, TerminalTurn) -> BoxFuture<'static, Result<(), HandlerError>>
        + Send
        + Sync,
>;

pub type DeferDecision =
    Arc<dyn Fn(ReviewedGoalResponse, Goal, ExtensionContext, TerminalTurn) -> Result<(), HandlerError> + Send + Sync>;

pub struct GoalAgentEndOptions {
    pub event: AgentEndEvent,
    pub ctx: ExtensionContext,
    pub review_goal: GoalEvidenceReview,
    pub select_goal: Arc<dyn Fn() -> Result<Option<Goal>, HandlerError> + Send + Sync>,
    pub is_same_goal: IsSameGoal,
    pub is_review_current: Arc<dyn Fn() -> Result<bool, HandlerError> + Send + Sync>,
    pub apply_decision: ApplyDecision,
    pub defer_decision: DeferDecision,
    pub defer_review: Arc<dyn Fn(Goal, ExtensionContext, TerminalTurn) -> Result<(), HandlerError> + Send + Sync>,
}

fn review_still_applies(options: &GoalAgentEndOptions, goal: &Goal) -> Result<bool, HandlerError> {
    Ok((options.is_review_current)()? && (options.is_same_goal)(options.ctx.clone(), goal)?)
}

/// `handleGoalAgentEnd`.
pub async fn handle_goal_agent_end(options: GoalAgentEndOptions) -> Result<(), HandlerError> {
    let Some(goal) = (options.select_goal)()? else {
        return Ok(());
    };
    if options.ctx.has_pending_messages()? {
        (options.defer_review)(goal, options.ctx.clone(), options.event.messages.clone())?;
        return Ok(());
    }
    let mut payload = pi_js_compat::js_json::JsObject::new();
    payload.insert(
        "objective".into(),
        pi_js_compat::js_json::JsValue::String(goal.objective.clone()),
    );
    let reviewed = (options.review_goal)(GoalReviewInput {
        kind: GoalReviewKind::GoalIdleReview,
        payload,
        ctx: options.ctx.clone(),
    })
    .await?;
    if !review_still_applies(&options, &goal)? {
        return Ok(());
    }
    if options.ctx.has_pending_messages()? {
        (options.defer_decision)(reviewed, goal, options.ctx.clone(), options.event.messages.clone())?;
        return Ok(());
    }
    (options.apply_decision)(reviewed, goal, options.ctx.clone(), options.event.messages.clone()).await
}
