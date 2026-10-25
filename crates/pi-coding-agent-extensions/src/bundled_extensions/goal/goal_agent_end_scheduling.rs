//! Port of `packages/coding-agent/extensions/goal/src/goal-agent-end-scheduling.ts`.

use std::sync::Arc;

use crate::core::extensions::types::{AgentEndEvent, BoxFuture, ExtensionContext, HandlerError};

use super::empty_response_scheduling::EmptyResponseScheduler;
use super::goal_agent_end::{GoalAgentEndOptions, IsSameGoal, handle_goal_agent_end};
use super::goal_scheduling::{GoalScheduler, TerminalTurn};
use super::goal_types::{Goal, GoalEvidenceReview, ReviewedGoalResponse};

pub type ApplyIdleDecision = Arc<
    dyn Fn(ReviewedGoalResponse, Goal, ExtensionContext, TerminalTurn) -> BoxFuture<'static, Result<(), HandlerError>>
        + Send
        + Sync,
>;

pub struct ScheduledGoalAgentEndOptions {
    pub event: AgentEndEvent,
    pub ctx: ExtensionContext,
    pub review_goal: GoalEvidenceReview,
    pub scheduler: GoalScheduler<Goal, ReviewedGoalResponse>,
    pub empty_response_scheduler: EmptyResponseScheduler<Goal>,
    pub apply_decision: ApplyIdleDecision,
    pub select_goal: Arc<dyn Fn() -> Result<Option<Goal>, HandlerError> + Send + Sync>,
    pub is_same_goal: IsSameGoal,
}

/// `runScheduledGoalAgentEnd`.
pub async fn run_scheduled_goal_agent_end(options: ScheduledGoalAgentEndOptions) -> Result<(), HandlerError> {
    let review_epoch = options.scheduler.capture_epoch(&options.ctx)?;
    let scheduler_for_current = options.scheduler.clone();
    let ctx_for_current = options.ctx.clone();
    let scheduler_for_defer_decision = options.scheduler.clone();
    let scheduler_for_defer_review = options.scheduler.clone();
    let apply_decision = Arc::clone(&options.apply_decision);
    let _ = options.empty_response_scheduler;
    handle_goal_agent_end(GoalAgentEndOptions {
        event: options.event,
        ctx: options.ctx,
        review_goal: options.review_goal,
        select_goal: options.select_goal,
        is_same_goal: options.is_same_goal,
        is_review_current: Arc::new(move || scheduler_for_current.is_epoch_current(&ctx_for_current, review_epoch)),
        apply_decision,
        defer_decision: Arc::new(move |decision, goal, ctx, terminal_turn| {
            scheduler_for_defer_decision.defer_decision(decision, goal, ctx, terminal_turn)
        }),
        defer_review: Arc::new(move |goal, ctx, terminal_turn| {
            scheduler_for_defer_review.defer_review(goal, ctx, terminal_turn)
        }),
    })
    .await
}
