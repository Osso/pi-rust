//! Port of `packages/coding-agent/extensions/goal/src/empty-response-scheduling.ts`.

use pi_js_compat::js_json::JsString;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::task::JoinHandle;

use crate::core::extensions::types::{ExtensionApi, ExtensionContext, UserMessageContent};

use super::stale_context::is_stale_context_error;

const EMPTY_RESPONSE_RETRY_DELAY_MS: u64 = 1_000;

pub struct EmptyResponseSchedulingOptions<TGoal> {
    pub pi: ExtensionApi,
    pub is_same_running_goal: Arc<
        dyn Fn(ExtensionContext, TGoal) -> Result<bool, crate::core::extensions::types::HandlerError> + Send + Sync,
    >,
}

#[derive(Clone)]
pub struct EmptyResponseScheduler<TGoal> {
    options: Arc<EmptyResponseSchedulingOptions<TGoal>>,
    timers: Arc<Mutex<HashMap<JsString, JoinHandle<()>>>>,
}

impl<TGoal> EmptyResponseScheduler<TGoal>
where
    TGoal: Clone + Send + Sync + 'static,
{
    pub fn clear_session(&self, session_id: &JsString) {
        let timer = self
            .timers
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(session_id);
        if let Some(timer) = timer {
            timer.abort();
        }
    }

    pub fn clear_all(&self) {
        let mut timers = self.timers.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        for (_, timer) in timers.drain() {
            timer.abort();
        }
    }

    pub fn schedule(&self, ctx: ExtensionContext, goal: TGoal) {
        let Ok(session) = ctx.session_manager() else {
            return;
        };
        let session_id = session.get_session_id().to_owned();
        self.clear_session(&session_id);
        let scheduler = self.clone();
        let task_session_id = session_id.clone();
        let handle = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(EMPTY_RESPONSE_RETRY_DELAY_MS)).await;
            scheduler
                .timers
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .remove(&task_session_id);
            let same_goal = match (scheduler.options.is_same_running_goal)(ctx.clone(), goal.clone()) {
                Ok(same_goal) => same_goal,
                Err(error) if is_stale_context_error(&error.to_string()) => return,
                Err(error) => crate::utils::uncaught::throw_uncaught(crate::utils::uncaught::ERROR, error.to_string()),
            };
            if !same_goal || ctx.has_pending_messages().unwrap_or(false) {
                return;
            }
            if !ctx.is_idle().unwrap_or(false) {
                scheduler.schedule(ctx, goal);
                return;
            }
            if let Err(error) = scheduler.options.pi.send_user_message(
                UserMessageContent::Text("Continue working toward the active goal.".to_owned().into()),
                None,
            ) && !is_stale_context_error(&error.to_string())
            {
                crate::utils::uncaught::throw_uncaught(crate::utils::uncaught::ERROR, error.to_string());
            }
        });
        self.timers
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(session_id, handle);
    }
}

/// `createEmptyResponseScheduler`.
pub fn create_empty_response_scheduler<TGoal>(
    options: EmptyResponseSchedulingOptions<TGoal>,
) -> EmptyResponseScheduler<TGoal>
where
    TGoal: Clone + Send + Sync + 'static,
{
    EmptyResponseScheduler {
        options: Arc::new(options),
        timers: Arc::new(Mutex::new(HashMap::new())),
    }
}
