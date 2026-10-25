//! Port of `packages/coding-agent/extensions/goal/src/goal-review-evidence.ts`.

use pi_js_compat::js_json::{JsObject, JsString, JsValue};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

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
    sessions: SharedUserEvidence,
}

#[derive(Clone, Default)]
struct UserEvidence {
    request: Option<JsString>,
    answers: Vec<JsString>,
}

type SharedUserEvidence = Arc<Mutex<HashMap<JsString, UserEvidence>>>;

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

fn answered_questions_text(event: &ToolResultEvent) -> Option<JsString> {
    if event.tool_name.as_str() != "ask_questions" || event.is_error {
        return None;
    }
    let details = event.details.as_ref()?;
    if !pi_js_compat::js_json::is_truthy(details)
        || details.get("cancelled").is_some_and(pi_js_compat::js_json::is_truthy)
        || details.get("queued").is_some_and(pi_js_compat::js_json::is_truthy)
    {
        return None;
    }
    let text = JsString::join(
        event.content.iter().filter_map(|part| match part {
            pi_ai::types::TextOrImageContent::Text(text) => Some(&text.text),
            pi_ai::types::TextOrImageContent::Image(_) => None,
        }),
        "\n",
    );
    (!text.is_empty()).then_some(text)
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

fn assemble_review_payload(
    mut payload: JsObject,
    user: UserEvidence,
    events: &[GoalReviewEvidenceEvent],
) -> Result<JsObject, HandlerError> {
    if let Some(request) = user.request {
        payload.insert("userRequest".into(), JsValue::String(request));
    }
    if !user.answers.is_empty() {
        payload.insert(
            "userAnswers".into(),
            JsValue::Array(user.answers.into_iter().map(JsValue::String).collect()),
        );
    }
    if !events.is_empty() {
        payload.insert(
            "conversationEvents".into(),
            pi_js_compat::js_json::to_value(events).map_err(|error| HandlerError::new(error.to_string()))?,
        );
    }
    Ok(payload)
}

fn create_evidence_review(review_goal: GoalSupervisorReview, sessions: SharedUserEvidence) -> GoalEvidenceReview {
    Arc::new(move |input: GoalReviewInput| {
        let review_goal = Arc::clone(&review_goal);
        let sessions = sessions.clone();
        Box::pin(async move {
            let cwd = input.ctx.cwd()?;
            let session = input.ctx.session_manager()?;
            let conversation_events = load_or_migrate_active_goal(&cwd, &session)?
                .and_then(|goal| goal.review_evidence)
                .unwrap_or_default();
            let user = sessions
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .get(&session.get_session_id())
                .cloned()
                .unwrap_or_default();
            let payload = assemble_review_payload(input.payload, user, &conversation_events)?;
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
    let sessions = SharedUserEvidence::default();
    GoalReviewEvidenceController {
        review: create_evidence_review(review_goal, sessions.clone()),
        sessions,
    }
}

impl GoalReviewEvidenceController {
    pub fn append_input(&self, event: &InputEvent, ctx: &ExtensionContext) -> Result<(), HandlerError> {
        if event.source != InputSource::Extension && !event.text.is_empty() {
            let session_id = ctx.session_manager()?.get_session_id();
            self.sessions
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .insert(
                    session_id,
                    UserEvidence {
                        request: Some(event.text.clone()),
                        answers: Vec::new(),
                    },
                );
        }
        append_input_evidence(event, ctx)
    }

    pub fn append_tool_result(&self, event: &ToolResultEvent, ctx: &ExtensionContext) -> Result<(), HandlerError> {
        append_end_turn_evidence(event, ctx)?;
        let Some(answers) = answered_questions_text(event) else {
            return Ok(());
        };
        let session_id = ctx.session_manager()?.get_session_id();
        self.sessions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .entry(session_id)
            .or_default()
            .answers
            .push(answers.clone());
        append_evidence(ctx, GoalReviewEvidenceEvent::User { text: answers })
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
