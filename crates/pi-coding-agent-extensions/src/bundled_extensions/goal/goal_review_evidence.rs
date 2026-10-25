//! Port of `packages/coding-agent/extensions/goal/src/goal-review-evidence.ts`.

use std::sync::Arc;

use crate::core::extensions::types::{
    BoxFuture, ExtensionContext, HandlerError, InputEvent, InputSource, ToolResultEvent,
};

use super::goal_state::{load_or_migrate_active_goal, save_goal};
use super::goal_types::{
    Goal, GoalEvidenceReview, GoalReviewEvidenceEvent, GoalReviewInput, GoalSupervisorReview, ReviewedGoalResponse,
};

#[derive(Clone)]
pub struct GoalReviewEvidenceController {
    pub review: GoalEvidenceReview,
}

fn append_evidence(ctx: &ExtensionContext, evidence: GoalReviewEvidenceEvent) -> Result<(), HandlerError> {
    let cwd = ctx.cwd()?;
    let session = ctx.session_manager()?;
    let Some(goal) = load_or_migrate_active_goal(&cwd, &session)? else {
        return Ok(());
    };
    let mut review_evidence = goal.review_evidence.clone().unwrap_or_default();
    review_evidence.push(evidence);
    save_goal(
        &session,
        &Goal {
            review_evidence: Some(review_evidence),
            ..goal
        },
    )
}

fn append_input_evidence(event: &InputEvent, ctx: &ExtensionContext) -> Result<(), HandlerError> {
    if event.source == InputSource::Extension || event.text.is_empty() {
        return Ok(());
    }
    append_evidence(
        ctx,
        GoalReviewEvidenceEvent::User {
            text: event.text.clone(),
        },
    )
}

fn append_end_turn_evidence(event: &ToolResultEvent, ctx: &ExtensionContext) -> Result<(), HandlerError> {
    if event.tool_name.as_str() != "end_turn" || event.is_error {
        return Ok(());
    }
    let Some(reason) = event
        .input
        .get("reason")
        .and_then(pi_js_compat::js_json::JsValue::as_string)
    else {
        return Ok(());
    };
    if reason.trim().is_empty() {
        return Ok(());
    }
    append_evidence(
        ctx,
        GoalReviewEvidenceEvent::EndTurn {
            reason: reason.to_owned(),
        },
    )
}

fn consume_evidence(ctx: &ExtensionContext, reviewed_goal: &Goal, evidence_count: usize) -> Result<(), HandlerError> {
    if evidence_count == 0 {
        return Ok(());
    }
    let cwd = ctx.cwd()?;
    let session = ctx.session_manager()?;
    let Some(goal) = load_or_migrate_active_goal(&cwd, &session)? else {
        return Ok(());
    };
    let is_reviewed_goal = goal.created_at == reviewed_goal.created_at && goal.objective == reviewed_goal.objective;
    let Some(evidence) = goal.review_evidence.clone() else {
        return Ok(());
    };
    if !is_reviewed_goal {
        return Ok(());
    }
    let remaining: Vec<_> = evidence.into_iter().skip(evidence_count).collect();
    save_goal(
        &session,
        &Goal {
            review_evidence: if remaining.is_empty() { None } else { Some(remaining) },
            ..goal
        },
    )
}

fn create_evidence_review(review_goal: GoalSupervisorReview) -> GoalEvidenceReview {
    Arc::new(move |input: GoalReviewInput| {
        let review_goal = Arc::clone(&review_goal);
        Box::pin(async move {
            let cwd = input.ctx.cwd()?;
            let session = input.ctx.session_manager()?;
            let conversation_events = load_or_migrate_active_goal(&cwd, &session)?
                .and_then(|goal| goal.review_evidence)
                .unwrap_or_default();
            let mut payload = input.payload.clone();
            if !conversation_events.is_empty() {
                payload.insert(
                    "conversationEvents".into(),
                    pi_js_compat::js_json::to_value(&conversation_events)
                        .map_err(|error| HandlerError::new(error.to_string()))?,
                );
            }
            let decision = review_goal(GoalReviewInput {
                kind: input.kind,
                payload,
                ctx: input.ctx,
            })
            .await?;
            Ok(ReviewedGoalResponse {
                decision,
                evidence_count: conversation_events.len(),
            })
        }) as BoxFuture<'static, Result<ReviewedGoalResponse, HandlerError>>
    })
}

/// `createGoalReviewEvidenceController`.
pub fn create_goal_review_evidence_controller(review_goal: GoalSupervisorReview) -> GoalReviewEvidenceController {
    GoalReviewEvidenceController {
        review: create_evidence_review(review_goal),
    }
}

impl GoalReviewEvidenceController {
    pub fn append_input(&self, event: &InputEvent, ctx: &ExtensionContext) -> Result<(), HandlerError> {
        append_input_evidence(event, ctx)
    }

    pub fn append_tool_result(&self, event: &ToolResultEvent, ctx: &ExtensionContext) -> Result<(), HandlerError> {
        append_end_turn_evidence(event, ctx)
    }

    pub fn consume(
        &self,
        ctx: &ExtensionContext,
        reviewed_goal: &Goal,
        evidence_count: usize,
    ) -> Result<(), HandlerError> {
        consume_evidence(ctx, reviewed_goal, evidence_count)
    }
}
