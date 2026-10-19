//! Port of `packages/coding-agent/extensions/goal/src/wait-countdown.ts`.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use pi_js_compat::date::date_parse_millis;
use tokio::task::JoinHandle;

use crate::core::extensions::types::ExtensionContext;
use crate::core::session_manager::lock_session_manager;

/// Countdown redraw scheduler for persisted Supervisor review deadlines.
#[derive(Clone, Default)]
pub struct WaitCountdownRefresher {
    timers: Arc<Mutex<HashMap<String, JoinHandle<()>>>>,
}

impl WaitCountdownRefresher {
    pub fn clear_all(&self) {
        let mut timers = self.timers.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        for (_, timer) in timers.drain() {
            timer.abort();
        }
    }

    pub fn clear_session(&self, session_id: &str) {
        let timer = self
            .timers
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(session_id);
        if let Some(timer) = timer {
            timer.abort();
        }
    }

    pub fn start(&self, ctx: ExtensionContext, review_at: &str) {
        let Ok(session) = ctx.session_manager() else {
            return;
        };
        let session_id = lock_session_manager(&session).get_session_id().to_owned();
        self.clear_session(&session_id);
        let Some(deadline_ms) = date_parse_millis(review_at) else {
            return;
        };
        if deadline_ms <= now_millis() {
            return;
        }
        self.schedule_next_render(ctx, session_id, deadline_ms);
    }

    fn schedule_next_render(&self, ctx: ExtensionContext, session_id: String, deadline_ms: i64) {
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
