//! Port of `packages/coding-agent/extensions/goal/src/goal-scheduling.ts`.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use pi_agent_core::AgentToolResult;
use pi_agent_core::agent::types::AgentMessage;
use pi_ai::types::TextOrImageContent;
use pi_js_compat::abort::AbortSignal;
use pi_js_compat::js_json::{self, JsObject as Map, JsString, JsValue as Value};
use tokio::task::JoinHandle;

use crate::core::extensions::types::{BoxFuture, ExtensionApi, ExtensionContext, HandlerError};

const PENDING_DECISION_RETRY_DELAY_MS: u64 = 1_000;
const WAIT_REVIEW_DELAY_MS: u64 = 25 * 60 * 1_000;

pub type TerminalTurn = Vec<AgentMessage>;

pub type GoalScheduledStatus = Arc<dyn Fn(&str) -> Result<(), HandlerError> + Send + Sync>;

#[derive(Clone)]
pub struct GoalWaitCallbacks {
    pub on_agent_wait: GoalScheduledStatus,
    pub on_agent_wake: Arc<dyn Fn() -> Result<(), HandlerError> + Send + Sync>,
    pub on_review_scheduled: GoalScheduledStatus,
}

#[derive(Clone)]
struct ScheduledWait {
    review_at: String,
    epoch: i64,
}

pub type ReviewGoalFn<TGoal, TDecision> = Arc<
    dyn Fn(ExtensionContext, TGoal, TerminalTurn, Option<Value>) -> BoxFuture<'static, Result<TDecision, HandlerError>>
        + Send
        + Sync,
>;

pub type ApplyDecisionFn<TGoal, TDecision> = Arc<
    dyn Fn(TDecision, TGoal, ExtensionContext, TerminalTurn) -> BoxFuture<'static, Result<(), HandlerError>>
        + Send
        + Sync,
>;

pub type IsSameRunningGoal<TGoal> = Arc<dyn Fn(ExtensionContext, TGoal) -> Result<bool, HandlerError> + Send + Sync>;

pub struct GoalSchedulingOptions<TGoal, TDecision> {
    pub pi: ExtensionApi,
    pub review_goal: ReviewGoalFn<TGoal, TDecision>,
    pub apply_decision: ApplyDecisionFn<TGoal, TDecision>,
    pub is_same_running_goal: IsSameRunningGoal<TGoal>,
    pub report_error: Arc<dyn Fn(HandlerError, ExtensionContext) + Send + Sync>,
}

struct GoalSchedulerInner<TGoal, TDecision> {
    options: GoalSchedulingOptions<TGoal, TDecision>,
    pending_decision_timers: Mutex<HashMap<JsString, JoinHandle<()>>>,
    cancellation_epochs: Mutex<HashMap<JsString, i64>>,
    wait_controllers: Mutex<HashMap<JsString, pi_js_compat::abort::AbortController>>,
    wait_review_timers: Mutex<HashMap<JsString, JoinHandle<()>>>,
}

#[derive(Clone)]
pub struct GoalScheduler<TGoal, TDecision> {
    inner: Arc<GoalSchedulerInner<TGoal, TDecision>>,
}

fn tool_error(result: &AgentToolResult, tool_name: &str) -> Option<HandlerError> {
    if result.is_error != Some(true) {
        return None;
    }
    let message = JsString::join(
        result
            .content
            .iter()
            .filter_map(|item| match item {
                TextOrImageContent::Text(text) => Some(text.text.clone()),
                TextOrImageContent::Image(_) => None,
            })
            .collect::<Vec<_>>(),
        "\n",
    );
    let trimmed = message.trim();
    if trimmed.is_empty() {
        return Some(HandlerError::new(format!("{tool_name} failed")));
    }
    Some(HandlerError::new(trimmed))
}

fn active_agent_count(details: Option<&Value>) -> f64 {
    let Some(Value::Object(details)) = details else {
        return 0.0;
    };
    details.get("activeCount").and_then(Value::as_f64).unwrap_or(0.0)
}

fn wait_wake_evidence(result: &AgentToolResult) -> Result<Value, HandlerError> {
    let mut evidence = match &result.details {
        Some(Value::Object(details)) => details.clone(),
        Some(other) => {
            let mut object = Map::new();
            object.insert("details".into(), other.clone());
            object
        }
        None => Map::new(),
    };
    evidence.insert(
        "content".into(),
        js_json::to_value(&result.content).map_err(|error| HandlerError::new(error.to_string()))?,
    );
    Ok(Value::Object(evidence))
}

fn session_id(ctx: &ExtensionContext) -> Result<JsString, HandlerError> {
    let session = ctx.session_manager()?;
    Ok(session.get_session_id().clone())
}

fn report<TGoal, TDecision>(
    options: &GoalSchedulingOptions<TGoal, TDecision>,
    error: HandlerError,
    ctx: ExtensionContext,
) {
    (options.report_error)(error, ctx);
}

impl<TGoal, TDecision> GoalScheduler<TGoal, TDecision>
where
    TGoal: Clone + Send + Sync + 'static,
    TDecision: Clone + Send + Sync + 'static,
{
    pub fn capture_epoch(&self, ctx: &ExtensionContext) -> Result<i64, HandlerError> {
        let session_id = session_id(ctx)?;
        let mut epochs = self
            .inner
            .cancellation_epochs
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        Ok(*epochs.entry(session_id).or_insert(0))
    }

    pub fn is_epoch_current(&self, ctx: &ExtensionContext, epoch: i64) -> Result<bool, HandlerError> {
        let session_id = session_id(ctx)?;
        let epochs = self
            .inner
            .cancellation_epochs
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        Ok(epochs.get(&session_id).copied().unwrap_or(0) == epoch)
    }

    pub fn clear_session(&self, session_id: &JsString) {
        {
            let mut epochs = self
                .inner
                .cancellation_epochs
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            *epochs.entry(session_id.to_owned()).or_insert(0) += 1;
        }
        clear_timer(&self.inner.pending_decision_timers, session_id);
        clear_timer(&self.inner.wait_review_timers, session_id);
        self.clear_wait(session_id);
    }

    pub fn clear_all(&self) {
        {
            let mut epochs = self
                .inner
                .cancellation_epochs
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            for epoch in epochs.values_mut() {
                *epoch += 1;
            }
        }
        clear_timers(&self.inner.pending_decision_timers);
        clear_timers(&self.inner.wait_review_timers);
        let mut controllers = self
            .inner
            .wait_controllers
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        for controller in controllers.values() {
            controller.abort(None);
        }
        controllers.clear();
    }

    pub fn defer_review(
        &self,
        goal: TGoal,
        ctx: ExtensionContext,
        terminal_turn: TerminalTurn,
    ) -> Result<(), HandlerError> {
        self.schedule_review_retry(ctx, goal, terminal_turn, None)
    }

    pub fn defer_decision(
        &self,
        decision: TDecision,
        goal: TGoal,
        ctx: ExtensionContext,
        terminal_turn: TerminalTurn,
    ) -> Result<(), HandlerError> {
        let session_id = session_id(&ctx)?;
        clear_timer(&self.inner.pending_decision_timers, &session_id);
        let scheduler = self.clone();
        let task_session_id = session_id.clone();
        let handle = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(PENDING_DECISION_RETRY_DELAY_MS)).await;
            scheduler
                .inner
                .pending_decision_timers
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .remove(&task_session_id);
            if let Err(error) = scheduler.apply_deferred_decision(decision, goal, ctx.clone(), terminal_turn) {
                report(&scheduler.inner.options, error, ctx);
            }
        });
        self.inner
            .pending_decision_timers
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(session_id, handle);
        Ok(())
    }

    pub async fn wait_for_agents_or_schedule_review(
        &self,
        ctx: ExtensionContext,
        goal: TGoal,
        terminal_turn: TerminalTurn,
        callbacks: GoalWaitCallbacks,
    ) -> Result<(), HandlerError> {
        let session_id = session_id(&ctx)?;
        let epoch = self
            .inner
            .cancellation_epochs
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(&session_id)
            .copied()
            .unwrap_or(0);
        let result = async {
            let listed = self
                .inner
                .options
                .pi
                .call_tool(
                    "list_agents".to_owned(),
                    serde_json::json!({"parentId":"main"}).into(),
                    None,
                    None,
                )
                .await?;
            if !self.epoch_matches(&session_id, epoch) {
                return Ok(());
            }
            if let Some(error) = tool_error(&listed, "list_agents") {
                return Err(error);
            }
            if active_agent_count(listed.details.as_ref()) == 0.0 {
                self.schedule_wait_review(
                    ctx.clone(),
                    goal.clone(),
                    terminal_turn.clone(),
                    &callbacks.on_review_scheduled,
                )?;
            } else {
                let scheduled = self.schedule_wait_review(
                    ctx.clone(),
                    goal.clone(),
                    terminal_turn.clone(),
                    &callbacks.on_agent_wait,
                )?;
                self.start_agent_wait(
                    ctx.clone(),
                    goal.clone(),
                    terminal_turn.clone(),
                    callbacks.clone(),
                    scheduled,
                )?;
            }
            Ok(())
        }
        .await;
        if let Err(error) = result {
            if !self.epoch_matches(&session_id, epoch) {
                return Ok(());
            }
            report(&self.inner.options, error, ctx.clone());
            self.schedule_wait_review(ctx, goal, terminal_turn, &callbacks.on_review_scheduled)?;
        }
        Ok(())
    }

    fn epoch_matches(&self, session_id: &JsString, epoch: i64) -> bool {
        let epochs = self
            .inner
            .cancellation_epochs
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        epochs.get(session_id).copied().unwrap_or(0) == epoch
    }

    fn clear_wait(&self, session_id: &JsString) {
        let controller = self
            .inner
            .wait_controllers
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(session_id);
        if let Some(controller) = controller {
            controller.abort(None);
        }
    }

    fn can_start_review(
        &self,
        ctx: &ExtensionContext,
        goal: &TGoal,
        terminal_turn: &TerminalTurn,
        wake_evidence: Option<&Value>,
    ) -> Result<bool, HandlerError> {
        if !(self.inner.options.is_same_running_goal)(ctx.clone(), goal.clone())? {
            return Ok(false);
        }
        if !ctx.has_pending_messages()? {
            return Ok(true);
        }
        self.schedule_review_retry(ctx.clone(), goal.clone(), terminal_turn.clone(), wake_evidence.cloned())?;
        Ok(false)
    }

    async fn apply_reviewed_decision(
        &self,
        decision: TDecision,
        goal: TGoal,
        ctx: ExtensionContext,
        terminal_turn: TerminalTurn,
    ) -> Result<(), HandlerError> {
        if ctx.has_pending_messages()? {
            return self.defer_decision(decision, goal, ctx, terminal_turn);
        }
        (self.inner.options.apply_decision)(decision, goal, ctx, terminal_turn).await
    }

    async fn review_and_apply(
        &self,
        ctx: ExtensionContext,
        goal: TGoal,
        terminal_turn: TerminalTurn,
        wake_evidence: Option<Value>,
    ) -> Result<(), HandlerError> {
        let review_epoch = self.capture_epoch(&ctx)?;
        if !self.can_start_review(&ctx, &goal, &terminal_turn, wake_evidence.as_ref())? {
            return Ok(());
        }
        let decision =
            (self.inner.options.review_goal)(ctx.clone(), goal.clone(), terminal_turn.clone(), wake_evidence).await?;
        let review_is_current = self.is_epoch_current(&ctx, review_epoch)?
            && (self.inner.options.is_same_running_goal)(ctx.clone(), goal.clone())?;
        if !review_is_current {
            return Ok(());
        }
        self.apply_reviewed_decision(decision, goal, ctx, terminal_turn).await
    }

    fn schedule_review_retry(
        &self,
        ctx: ExtensionContext,
        goal: TGoal,
        terminal_turn: TerminalTurn,
        wake_evidence: Option<Value>,
    ) -> Result<(), HandlerError> {
        let session_id = session_id(&ctx)?;
        clear_timer(&self.inner.wait_review_timers, &session_id);
        let scheduler = self.clone();
        let task_session_id = session_id.clone();
        let handle = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(PENDING_DECISION_RETRY_DELAY_MS)).await;
            scheduler
                .inner
                .wait_review_timers
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .remove(&task_session_id);
            let result = async {
                if !ctx.is_idle()? {
                    return scheduler.schedule_review_retry(ctx.clone(), goal, terminal_turn, wake_evidence);
                }
                scheduler
                    .review_and_apply(ctx.clone(), goal, terminal_turn, wake_evidence)
                    .await
            }
            .await;
            if let Err(error) = result {
                report(&scheduler.inner.options, error, ctx);
            }
        });
        self.inner
            .wait_review_timers
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(session_id, handle);
        Ok(())
    }

    fn schedule_wait_review(
        &self,
        ctx: ExtensionContext,
        goal: TGoal,
        terminal_turn: TerminalTurn,
        on_scheduled: &GoalScheduledStatus,
    ) -> Result<ScheduledWait, HandlerError> {
        let session_id = session_id(&ctx)?;
        let epoch = self.capture_epoch(&ctx)?;
        clear_timer(&self.inner.wait_review_timers, &session_id);
        let review_at = crate::utils::iso_time::format_iso_millis(now_millis() + WAIT_REVIEW_DELAY_MS as i64);
        let scheduler = self.clone();
        let task_session_id = session_id.clone();
        let handle = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(WAIT_REVIEW_DELAY_MS)).await;
            let result = async {
                if !scheduler.is_epoch_current(&ctx, epoch)? {
                    return Ok(());
                }
                // A running timeout is no longer abortable like a pending JS timer.
                scheduler
                    .inner
                    .wait_review_timers
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .remove(&task_session_id);
                if !scheduler.claim_wait_review(&ctx, epoch)? {
                    return Ok(());
                }
                if !ctx.is_idle()? {
                    return scheduler.schedule_review_retry(ctx.clone(), goal, terminal_turn, None);
                }
                scheduler.review_and_apply(ctx.clone(), goal, terminal_turn, None).await
            }
            .await;
            if let Err(error) = result {
                report(&scheduler.inner.options, error, ctx);
            }
        });
        self.inner
            .wait_review_timers
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(session_id, handle);
        on_scheduled(&review_at)?;
        Ok(ScheduledWait { review_at, epoch })
    }

    fn claim_wait_review(&self, ctx: &ExtensionContext, expected_epoch: i64) -> Result<bool, HandlerError> {
        let session_id = session_id(ctx)?;
        let mut epochs = self
            .inner
            .cancellation_epochs
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if epochs.get(&session_id).copied().unwrap_or(0) != expected_epoch {
            return Ok(false);
        }
        epochs.insert(session_id.clone(), expected_epoch + 1);
        drop(epochs);
        clear_timer(&self.inner.wait_review_timers, &session_id);
        self.clear_wait(&session_id);
        Ok(true)
    }

    fn start_agent_wait(
        &self,
        ctx: ExtensionContext,
        goal: TGoal,
        terminal_turn: TerminalTurn,
        callbacks: GoalWaitCallbacks,
        scheduled: ScheduledWait,
    ) -> Result<(), HandlerError> {
        let session_id = session_id(&ctx)?;
        self.clear_wait(&session_id);
        let controller = pi_js_compat::abort::AbortController::new();
        self.inner
            .wait_controllers
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(session_id.clone(), controller.clone());
        let scheduler = self.clone();
        tokio::spawn(async move {
            let result = scheduler
                .wait_for_agent_wake(
                    ctx.clone(),
                    goal,
                    terminal_turn,
                    controller.signal(),
                    &scheduled,
                    &callbacks,
                )
                .await;
            if let Err(error) = result {
                if controller.signal().aborted() || !scheduler.epoch_matches(&session_id, scheduled.epoch) {
                    return;
                }
                let mut controllers = scheduler
                    .inner
                    .wait_controllers
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                if controllers.get(&session_id) == Some(&controller) {
                    controllers.remove(&session_id);
                }
                drop(controllers);
                report(&scheduler.inner.options, error, ctx.clone());
                if let Err(error) = (callbacks.on_review_scheduled)(&scheduled.review_at) {
                    report(&scheduler.inner.options, error, ctx);
                }
            }
        });
        Ok(())
    }

    async fn wait_for_agent_wake(
        &self,
        ctx: ExtensionContext,
        goal: TGoal,
        terminal_turn: TerminalTurn,
        signal: AbortSignal,
        scheduled: &ScheduledWait,
        callbacks: &GoalWaitCallbacks,
    ) -> Result<(), HandlerError> {
        let result = self
            .inner
            .options
            .pi
            .call_tool(
                "wait_agent".to_owned(),
                Value::Object(Map::new()),
                Some(signal.clone()),
                None,
            )
            .await?;
        if signal.aborted() || !self.is_epoch_current(&ctx, scheduled.epoch)? {
            return Ok(());
        }
        if let Some(error) = tool_error(&result, "wait_agent") {
            return Err(error);
        }
        if !self.claim_wait_review(&ctx, scheduled.epoch)? {
            return Ok(());
        }
        (callbacks.on_agent_wake)()?;
        self.review_and_apply(ctx, goal, terminal_turn, Some(wait_wake_evidence(&result)?))
            .await
    }

    fn apply_deferred_decision(
        &self,
        decision: TDecision,
        goal: TGoal,
        ctx: ExtensionContext,
        terminal_turn: TerminalTurn,
    ) -> Result<(), HandlerError> {
        if !(self.inner.options.is_same_running_goal)(ctx.clone(), goal.clone())? {
            return Ok(());
        }
        if !ctx.is_idle()? || ctx.has_pending_messages()? {
            return self.defer_decision(decision, goal, ctx, terminal_turn);
        }
        let scheduler = self.clone();
        tokio::spawn(async move {
            if let Err(error) =
                (scheduler.inner.options.apply_decision)(decision, goal, ctx.clone(), terminal_turn).await
            {
                report(&scheduler.inner.options, error, ctx);
            }
        });
        Ok(())
    }
}

fn clear_timer(timers: &Mutex<HashMap<JsString, JoinHandle<()>>>, session_id: &JsString) {
    let timer = timers
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .remove(session_id);
    if let Some(timer) = timer {
        timer.abort();
    }
}

fn clear_timers(timers: &Mutex<HashMap<JsString, JoinHandle<()>>>) {
    let mut timers = timers.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    for (_, timer) in timers.drain() {
        timer.abort();
    }
}

fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

/// `createGoalScheduler`.
pub fn create_goal_scheduler<TGoal, TDecision>(
    options: GoalSchedulingOptions<TGoal, TDecision>,
) -> GoalScheduler<TGoal, TDecision>
where
    TGoal: Clone + Send + Sync + 'static,
    TDecision: Clone + Send + Sync + 'static,
{
    GoalScheduler {
        inner: Arc::new(GoalSchedulerInner {
            options,
            pending_decision_timers: Mutex::new(HashMap::new()),
            cancellation_epochs: Mutex::new(HashMap::new()),
            wait_controllers: Mutex::new(HashMap::new()),
            wait_review_timers: Mutex::new(HashMap::new()),
        }),
    }
}
