//! Port of `packages/coding-agent/extensions/goal/src/completion-scheduling.ts`.

use pi_js_compat::js_json::JsString;
use std::sync::Arc;

use crate::core::extensions::types::{BoxFuture, ExtensionApi, ExtensionContext, HandlerError};
use crate::core::session_control_db::SupervisorResponse;

use super::goal_scheduling::{GoalScheduler, GoalSchedulingOptions, GoalWaitCallbacks, create_goal_scheduler};
use super::goal_types::{Goal, GoalEvidenceReview, GoalReviewInput, GoalReviewKind, ReviewedGoalResponse};

#[derive(Debug, Clone, PartialEq)]
pub struct CompletionWait {
    pub goal: Goal,
    pub completion_report: pi_js_compat::js_json::JsString,
}

pub type StatusAppend = Arc<
    dyn Fn(ExtensionContext, &pi_js_compat::js_json::JsString, Option<&str>) -> Result<(), HandlerError> + Send + Sync,
>;

pub struct CompletionSchedulingOptions {
    pub pi: ExtensionApi,
    pub review_goal: GoalEvidenceReview,
    pub consume_review_evidence: Arc<dyn Fn(ExtensionContext, Goal, usize) -> Result<(), HandlerError> + Send + Sync>,
    pub is_same_goal: Arc<dyn Fn(ExtensionContext, CompletionWait) -> Result<bool, HandlerError> + Send + Sync>,
    pub on_complete: Arc<dyn Fn(CompletionWait, ExtensionContext) + Send + Sync>,
    pub on_continue: Arc<dyn Fn(&pi_js_compat::js_json::JsString) + Send + Sync>,
    pub on_status: StatusAppend,
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

    /// `createReviewGuard`.
    pub fn create_review_guard(&self, ctx: &ExtensionContext) -> Box<dyn Fn() -> bool + Send + Sync> {
        let Ok(epoch) = self.scheduler.capture_epoch(ctx) else {
            return Box::new(|| false);
        };
        let scheduler = self.scheduler.clone();
        let ctx = ctx.clone();
        Box::new(move || scheduler.is_epoch_current(&ctx, epoch).unwrap_or(false))
    }

    /// `wait`.
    pub async fn wait(
        &self,
        goal: Goal,
        ctx: ExtensionContext,
        completion_report: &pi_js_compat::js_json::JsString,
        status_reason: &pi_js_compat::js_json::JsString,
    ) -> Result<(), HandlerError> {
        let message = pi_js_compat::js_json::JsString::from("Waiting: ") + status_reason;
        let waiting = CompletionWait {
            goal,
            completion_report: completion_report.to_owned(),
        };
        let options = Arc::clone(&self.options);
        let on_agent_wait = {
            let options = Arc::clone(&options);
            let ctx = ctx.clone();
            let message = message.clone();
            Arc::new(move || {
                let _ = (options.on_status)(ctx.clone(), &message, None);
            }) as Arc<dyn Fn() + Send + Sync>
        };
        let on_review_scheduled = {
            let options = Arc::clone(&options);
            let ctx = ctx.clone();
            let message = message.clone();
            Arc::new(move |review_at: &str| {
                let _ = (options.on_status)(ctx.clone(), &message, Some(review_at));
            }) as Arc<dyn Fn(&str) + Send + Sync>
        };
        self.scheduler
            .wait_for_agents_or_schedule_review(
                ctx,
                waiting,
                Vec::new(),
                GoalWaitCallbacks {
                    on_agent_wait,
                    on_review_scheduled,
                },
            )
            .await;
        Ok(())
    }
}

async fn apply_completion_decision(
    options: &CompletionSchedulingOptions,
    scheduler: &GoalScheduler<CompletionWait, ReviewedGoalResponse>,
    reviewed: ReviewedGoalResponse,
    waiting: CompletionWait,
    ctx: ExtensionContext,
) -> Result<(), HandlerError> {
    if let Ok(session) = ctx.session_manager() {
        scheduler.clear_session(&session.get_session_id());
    }
    match &reviewed.decision {
        SupervisorResponse::Complete { .. } => (options.on_complete)(waiting.clone(), ctx.clone()),
        SupervisorResponse::Continue { instructions, .. } => {
            (options.on_continue)(&pi_js_compat::js_json::JsString::from(instructions))
        }
        SupervisorResponse::Wait { reason } => {
            let message = pi_js_compat::js_json::JsString::from("Waiting: ") + reason;
            let on_agent_wait = {
                let on_status = Arc::clone(&options.on_status);
                let ctx = ctx.clone();
                let message = message.clone();
                Arc::new(move || {
                    let _ = on_status(ctx.clone(), &message, None);
                }) as Arc<dyn Fn() + Send + Sync>
            };
            let on_review_scheduled = {
                let on_status = Arc::clone(&options.on_status);
                let ctx = ctx.clone();
                let message = message.clone();
                Arc::new(move |review_at: &str| {
                    let _ = on_status(ctx.clone(), &message, Some(review_at));
                }) as Arc<dyn Fn(&str) + Send + Sync>
            };
            scheduler
                .wait_for_agents_or_schedule_review(
                    ctx.clone(),
                    waiting.clone(),
                    Vec::new(),
                    GoalWaitCallbacks {
                        on_agent_wait,
                        on_review_scheduled,
                    },
                )
                .await;
        }
        SupervisorResponse::Pause { reason } => {
            let _ = (options.on_status)(
                ctx.clone(),
                &(pi_js_compat::js_json::JsString::from("Goal waiting: ") + reason),
                None,
            );
        }
        SupervisorResponse::Error { reason } => {
            let _ = (options.on_status)(
                ctx.clone(),
                &(pi_js_compat::js_json::JsString::from("Goal review failed: ") + reason),
                None,
            );
            return Ok(());
        }
        SupervisorResponse::Set { reason, .. } => {
            let _ = (options.on_status)(
                ctx.clone(),
                &(pi_js_compat::js_json::JsString::from("Goal review failed: unexpected set decision: ") + reason),
                None,
            );
            return Ok(());
        }
        _ => {}
    }
    (options.consume_review_evidence)(ctx, waiting.goal, reviewed.evidence_count)
}

/// `createCompletionWaitScheduler`.
pub fn create_completion_wait_scheduler(options: CompletionSchedulingOptions) -> CompletionWaitScheduler {
    let options = Arc::new(options);
    let review_options = Arc::clone(&options);
    let options_for_apply = Arc::clone(&options);
    // The scheduler is created once and then shared by the apply closure through
    // the outer `OnceLock`; TypeScript uses the same late-bound `scheduler` variable.
    let scheduler_slot: Arc<std::sync::OnceLock<GoalScheduler<CompletionWait, ReviewedGoalResponse>>> =
        Arc::new(std::sync::OnceLock::new());
    let apply_slot = Arc::clone(&scheduler_slot);
    let scheduler_options = GoalSchedulingOptions {
        pi: options.pi.clone(),
        review_goal: Arc::new(move |ctx, waiting: CompletionWait, _terminal_turn, wake_evidence| {
            let options = Arc::clone(&review_options);
            Box::pin(async move {
                let mut payload = pi_js_compat::js_json::JsObject::new();
                payload.insert(
                    "objective".into(),
                    pi_js_compat::js_json::JsValue::String(waiting.goal.objective.clone()),
                );
                payload.insert(
                    "completionReport".into(),
                    pi_js_compat::js_json::JsValue::String(waiting.completion_report.clone()),
                );
                if let Some(wake_evidence) = wake_evidence {
                    payload.insert("wakeEvidence".into(), wake_evidence);
                }
                (options.review_goal)(GoalReviewInput {
                    kind: GoalReviewKind::GoalCompletionReview,
                    payload,
                    ctx,
                })
                .await
            }) as BoxFuture<'static, Result<ReviewedGoalResponse, HandlerError>>
        }),
        apply_decision: Arc::new(move |decision, waiting, ctx, _terminal_turn| {
            let options = Arc::clone(&options_for_apply);
            let apply_slot = Arc::clone(&apply_slot);
            Box::pin(async move {
                let scheduler = apply_slot.get().expect("completion scheduler initialized");
                apply_completion_decision(&options, scheduler, decision, waiting, ctx).await
            }) as BoxFuture<'static, Result<(), HandlerError>>
        }),
        is_same_running_goal: {
            let is_same_goal = Arc::clone(&options.is_same_goal);
            Arc::new(move |ctx, waiting| is_same_goal(ctx, waiting))
        },
        report_error: {
            let on_error = Arc::clone(&options.on_error);
            Arc::new(move |error, ctx| on_error(error, ctx))
        },
    };
    let created = create_goal_scheduler(scheduler_options);
    let _ = scheduler_slot.set(created.clone());
    CompletionWaitScheduler {
        options,
        scheduler: created,
    }
}
