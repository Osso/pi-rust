//! Port of `packages/coding-agent/extensions/goal/src/completion-scheduling.ts`.

use super::goal_scheduling::{GoalScheduler, GoalSchedulingOptions, GoalWaitCallbacks, create_goal_scheduler};
use super::goal_types::{
    Goal, GoalEvidenceReview, GoalReviewInput, GoalReviewKind, GoalSupervisorResponse, ReviewedGoalResponse,
};
use super::rendering::AppendSupervisorStatus;
use super::wait_status::create_wait_status_callbacks;
use crate::core::extensions::types::{BoxFuture, ExtensionApi, ExtensionContext, HandlerError};
use pi_js_compat::js_json::{JsObject, JsString, JsValue};
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq)]
pub struct CompletionWait {
    pub goal: Goal,
    pub completion_report: JsString,
}

pub type ContinueCompletion = Arc<dyn Fn(&JsString, Option<&JsString>) -> Result<(), HandlerError> + Send + Sync>;

pub struct CompletionSchedulingOptions {
    pub pi: ExtensionApi,
    pub review_goal: GoalEvidenceReview,
    pub consume_review_evidence: Arc<dyn Fn(ExtensionContext, Goal, usize) -> Result<(), HandlerError> + Send + Sync>,
    pub is_same_goal: Arc<dyn Fn(ExtensionContext, CompletionWait) -> Result<bool, HandlerError> + Send + Sync>,
    pub on_complete: Arc<dyn Fn(CompletionWait, ExtensionContext) -> Result<(), HandlerError> + Send + Sync>,
    pub on_continue: ContinueCompletion,
    pub on_status: AppendSupervisorStatus,
    pub on_clear_status: Arc<dyn Fn(&JsString) + Send + Sync>,
    pub on_error: Arc<dyn Fn(HandlerError, ExtensionContext) + Send + Sync>,
}

#[derive(Clone)]
pub struct CompletionWaitScheduler {
    options: Arc<CompletionSchedulingOptions>,
    scheduler: GoalScheduler<CompletionWait, ReviewedGoalResponse>,
}

impl CompletionWaitScheduler {
    pub fn clear_all(&self) {
        self.scheduler.clear_all();
    }
    pub fn clear_session(&self, session_id: &JsString) {
        self.scheduler.clear_session(session_id);
    }

    pub fn create_review_guard(
        &self,
        ctx: &ExtensionContext,
    ) -> Result<Box<dyn Fn() -> Result<bool, HandlerError> + Send + Sync>, HandlerError> {
        let epoch = self.scheduler.capture_epoch(ctx)?;
        let scheduler = self.scheduler.clone();
        let ctx = ctx.clone();
        Ok(Box::new(move || scheduler.is_epoch_current(&ctx, epoch)))
    }

    pub async fn wait(
        &self,
        goal: Goal,
        ctx: ExtensionContext,
        completion_report: &JsString,
        status_reason: &JsString,
        display_id: Option<&JsString>,
    ) -> Result<(), HandlerError> {
        let waiting = CompletionWait {
            goal,
            completion_report: completion_report.clone(),
        };
        let callbacks = completion_wait_callbacks(
            &self.options,
            &ctx,
            JsString::from("Waiting: ") + status_reason,
            display_id,
        );
        self.scheduler
            .wait_for_agents_or_schedule_review(ctx, waiting, Vec::new(), callbacks)
            .await
    }
}

fn completion_wait_callbacks(
    options: &CompletionSchedulingOptions,
    ctx: &ExtensionContext,
    message: JsString,
    display_id: Option<&JsString>,
) -> GoalWaitCallbacks {
    let append = options.on_status.clone();
    let display_id = display_id.cloned();
    let grouped: AppendSupervisorStatus =
        Arc::new(move |ctx, message, review_at, _| append(ctx, message, review_at, display_id.as_ref()));
    let clear = options.on_clear_status.clone();
    let wake_ctx = ctx.clone();
    create_wait_status_callbacks(
        grouped,
        ctx.clone(),
        message,
        Arc::new(move || {
            clear(&wake_ctx.session_manager()?.get_session_id());
            Ok(())
        }),
    )
}

async fn apply_completion_decision(
    options: &CompletionSchedulingOptions,
    scheduler: &GoalScheduler<CompletionWait, ReviewedGoalResponse>,
    reviewed: ReviewedGoalResponse,
    waiting: CompletionWait,
    ctx: ExtensionContext,
) -> Result<(), HandlerError> {
    scheduler.clear_session(&ctx.session_manager()?.get_session_id());
    let display_id = reviewed.decision.display_id();
    match &reviewed.decision {
        GoalSupervisorResponse::Complete { .. } => (options.on_complete)(waiting.clone(), ctx.clone())?,
        GoalSupervisorResponse::Continue { instructions, .. } => (options.on_continue)(instructions, display_id)?,
        GoalSupervisorResponse::Wait { reason, .. } => {
            let callbacks = completion_wait_callbacks(options, &ctx, JsString::from("Waiting: ") + reason, display_id);
            scheduler
                .wait_for_agents_or_schedule_review(ctx.clone(), waiting.clone(), Vec::new(), callbacks)
                .await?;
        }
        GoalSupervisorResponse::Pause { reason, .. } => (options.on_status)(
            ctx.clone(),
            &(JsString::from("Goal waiting: ") + reason),
            None,
            display_id,
        )?,
        GoalSupervisorResponse::Error { reason, .. } => {
            return (options.on_status)(
                ctx,
                &(JsString::from("Goal review failed: ") + reason),
                None,
                display_id,
            );
        }
        GoalSupervisorResponse::Set { reason, .. } => {
            return (options.on_status)(
                ctx,
                &(JsString::from("Goal review failed: unexpected set decision: ") + reason),
                None,
                display_id,
            );
        }
    }
    (options.consume_review_evidence)(ctx, waiting.goal, reviewed.evidence_count)
}

pub fn create_completion_wait_scheduler(options: CompletionSchedulingOptions) -> CompletionWaitScheduler {
    let options = Arc::new(options);
    let review_options = options.clone();
    let apply_options = options.clone();
    let scheduler_slot = Arc::new(std::sync::OnceLock::<GoalScheduler<CompletionWait, ReviewedGoalResponse>>::new());
    let apply_slot = scheduler_slot.clone();
    let scheduler = create_goal_scheduler(GoalSchedulingOptions {
        pi: options.pi.clone(),
        review_goal: Arc::new(move |ctx, waiting: CompletionWait, _turn, wake_evidence| {
            let options = review_options.clone();
            Box::pin(async move {
                let mut payload = JsObject::new();
                payload.insert("objective".into(), JsValue::String(waiting.goal.objective));
                payload.insert("completionReport".into(), JsValue::String(waiting.completion_report));
                if let Some(evidence) = wake_evidence {
                    payload.insert("wakeEvidence".into(), evidence);
                }
                (options.review_goal)(GoalReviewInput {
                    kind: GoalReviewKind::GoalCompletionReview,
                    ctx,
                    payload,
                })
                .await
            }) as BoxFuture<'static, Result<ReviewedGoalResponse, HandlerError>>
        }),
        apply_decision: Arc::new(move |decision, waiting, ctx, _turn| {
            let options = apply_options.clone();
            let slot = apply_slot.clone();
            Box::pin(async move {
                let scheduler = slot
                    .get()
                    .ok_or_else(|| HandlerError::new("Completion scheduler is not initialized"))?;
                apply_completion_decision(&options, scheduler, decision, waiting, ctx).await
            })
        }),
        is_same_running_goal: options.is_same_goal.clone(),
        report_error: options.on_error.clone(),
    });
    let _ = scheduler_slot.set(scheduler.clone());
    CompletionWaitScheduler { options, scheduler }
}
