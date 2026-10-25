//! Port of `packages/coding-agent/extensions/goal/src/error-status-scheduling.ts`.

use pi_js_compat::js_json::JsString;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::task::JoinHandle;

use crate::core::extensions::types::{ExtensionContext, HandlerError};

use super::stale_context::is_stale_context_error;

const ERROR_STATUS_SETTLEMENT_MS: u64 = 10;

pub type OnStatus =
    Arc<dyn Fn(ExtensionContext, &JsString) -> Result<(), crate::core::extensions::types::HandlerError> + Send + Sync>;

pub struct ErrorStatusSchedulingOptions {
    pub on_status: OnStatus,
}

#[derive(Clone)]
pub struct ErrorStatusScheduler {
    options: Arc<ErrorStatusSchedulingOptions>,
    timers: Arc<Mutex<HashMap<JsString, JoinHandle<()>>>>,
}

impl ErrorStatusScheduler {
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

    fn schedule_status_check(&self, ctx: ExtensionContext, message: JsString, session_id: JsString, delay_ms: u64) {
        let scheduler = self.clone();
        let task_session_id = session_id.clone();
        let handle = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(delay_ms)).await;
            scheduler.check_status(ctx, message, task_session_id);
        });
        self.timers
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(session_id, handle);
    }

    fn check_status(&self, ctx: ExtensionContext, message: JsString, session_id: JsString) {
        if ctx.has_pending_messages().unwrap_or(false) {
            self.timers
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .remove(&session_id);
            return;
        }
        if ctx.has_active_retry().unwrap_or(false) || !ctx.is_idle().unwrap_or(false) {
            self.schedule_status_check(ctx, message, session_id, ERROR_STATUS_SETTLEMENT_MS);
            return;
        }
        self.timers
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(&session_id);
        if let Err(error) = (self.options.on_status)(ctx, &message)
            && !is_stale_context_error(&error.to_string())
        {
            crate::utils::uncaught::throw_uncaught(crate::utils::uncaught::ERROR, error.to_string());
        }
    }

    pub fn schedule(&self, ctx: ExtensionContext, message: &JsString) -> Result<(), HandlerError> {
        let session = ctx.session_manager()?;
        let session_id = session.get_session_id().clone();
        self.clear_session(&session_id);
        let settings = ctx.settings_manager()?;
        let retry_delay_ms = match settings {
            Some(settings) => {
                settings
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .get_retry_settings()
                    .map_err(|failure| HandlerError::new(failure.to_string()))?
                    .base_delay_ms as u64
            }
            None => 0,
        };
        self.schedule_status_check(
            ctx,
            message.clone(),
            session_id,
            retry_delay_ms + ERROR_STATUS_SETTLEMENT_MS,
        );
        Ok(())
    }
}

/// `createErrorStatusScheduler`.
pub fn create_error_status_scheduler(options: ErrorStatusSchedulingOptions) -> ErrorStatusScheduler {
    ErrorStatusScheduler {
        options: Arc::new(options),
        timers: Arc::new(Mutex::new(HashMap::new())),
    }
}
