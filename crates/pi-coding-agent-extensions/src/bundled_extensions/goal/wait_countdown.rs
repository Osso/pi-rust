//! Port of `packages/coding-agent/extensions/goal/src/wait-countdown.ts`.

use pi_js_compat::js_json::JsString;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use pi_js_compat::date::date_parse_utf16;
use tokio::task::JoinHandle;

use crate::core::extensions::types::ExtensionContext;

/// Countdown redraw scheduler for persisted Supervisor review deadlines.
#[derive(Clone, Default)]
pub struct WaitCountdownRefresher {
    timers: Arc<Mutex<HashMap<JsString, JoinHandle<()>>>>,
}

impl WaitCountdownRefresher {
    pub fn clear_all(&self) {
        let mut timers = self.timers.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        for (_, timer) in timers.drain() {
            timer.abort();
        }
    }

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

    pub fn start(&self, ctx: ExtensionContext, review_at: &JsString) {
        let Ok(session) = ctx.session_manager() else {
            return;
        };
        let session_id = session.get_session_id().clone();
        self.clear_session(&session_id);
        let Some(deadline_ms) = parse_review_deadline(review_at) else {
            return;
        };
        if deadline_ms <= now_millis() {
            return;
        }
        self.schedule_next_render(ctx, session_id, deadline_ms);
    }

    fn schedule_next_render(&self, ctx: ExtensionContext, session_id: JsString, deadline_ms: i64) {
        let now = now_millis();
        let remaining_ms = deadline_ms - now;
        if remaining_ms <= 0 {
            return;
        }
        let remaining_seconds = (remaining_ms + 999) / 1_000;
        let next_change_at = deadline_ms - (remaining_seconds - 1) * 1_000;
        let delay = Duration::from_millis(u64::try_from((next_change_at - now).max(1)).unwrap_or(1));
        let timers = Arc::clone(&self.timers);
        let task_session_id = session_id.clone();
        let handle = tokio::spawn(async move {
            tokio::time::sleep(delay).await;
            timers
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .remove(&task_session_id);
            if let Ok(ui) = ctx.ui() {
                ui.request_render();
            }
            let refresher = WaitCountdownRefresher {
                timers: Arc::clone(&timers),
            };
            refresher.schedule_next_render(ctx, task_session_id, deadline_ms);
        });
        self.timers
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(session_id, handle);
    }
}

/// `Date.parse(reviewAt)` as epoch milliseconds; `None` where JavaScript returns `NaN`.
pub(super) fn parse_review_deadline(review_at: &JsString) -> Option<i64> {
    let millis = date_parse_utf16(review_at.units());
    (!millis.is_nan()).then_some(millis as i64)
}

/// `createWaitCountdownRefresher`.
pub fn create_wait_countdown_refresher() -> WaitCountdownRefresher {
    WaitCountdownRefresher::default()
}

fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}
