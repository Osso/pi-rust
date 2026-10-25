//! Port of `packages/coding-agent/extensions/goal/src/wait-countdown.ts`.

use pi_js_compat::callback_error::CallbackError;
use pi_js_compat::date::date_parse_utf16;
use pi_js_compat::js_json::JsString;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;

use crate::core::extensions::types::{ExtensionContext, HandlerError};

type BindingKey = (JsString, JsString);
type RenderRequest = oneshot::Sender<bool>;

struct RenderBinding {
    token: Arc<()>,
    requests: mpsc::UnboundedSender<RenderRequest>,
}

#[derive(Default)]
struct CountdownState {
    timers: HashMap<JsString, JoinHandle<()>>,
    bindings: HashMap<BindingKey, RenderBinding>,
}

/// Timers stay Send; scoped renderer callbacks run only on their owning LocalSet.
#[derive(Clone, Default)]
pub struct WaitCountdownRefresher {
    state: Arc<Mutex<CountdownState>>,
}

impl WaitCountdownRefresher {
    pub fn bind(
        &self,
        session_id: JsString,
        review_at: JsString,
        request_render: Rc<dyn Fn() -> Result<bool, CallbackError>>,
    ) -> Rc<dyn Fn()> {
        let key = (session_id, review_at);
        let token = Arc::new(());
        let (requests, mut receiver) = mpsc::unbounded_channel::<RenderRequest>();
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .bindings
            .insert(
                key.clone(),
                RenderBinding {
                    token: token.clone(),
                    requests,
                },
            );
        let task = tokio::task::spawn_local(async move {
            while let Some(reply) = receiver.recv().await {
                let rendered =
                    request_render().unwrap_or_else(|error| crate::utils::uncaught::throw_callback_uncaught(error));
                // A cancelled timer no longer needs the repaint result.
                let _ = reply.send(rendered);
            }
        });
        let state = self.state.clone();
        Rc::new(move || {
            let mut state = state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            if state
                .bindings
                .get(&key)
                .is_some_and(|binding| Arc::ptr_eq(&binding.token, &token))
            {
                state.bindings.remove(&key);
            }
            task.abort();
        })
    }

    pub fn clear_all(&self) {
        let mut state = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        for (_, timer) in state.timers.drain() {
            timer.abort();
        }
        state.bindings.clear();
    }

    pub fn clear_session(&self, session_id: &JsString) {
        let mut state = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(timer) = state.timers.remove(session_id) {
            timer.abort();
        }
        state.bindings.retain(|(session, _), _| session != session_id);
    }

    pub fn start(&self, ctx: ExtensionContext, review_at: &JsString) -> Result<(), HandlerError> {
        let session_id = ctx.session_manager()?.get_session_id().clone();
        self.start_session(session_id, review_at.clone());
        Ok(())
    }

    fn start_session(&self, session_id: JsString, review_at: JsString) {
        let mut state = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(timer) = state.timers.remove(&session_id) {
            timer.abort();
        }
        let Some(deadline) = parse_review_deadline(&review_at).filter(|deadline| *deadline > now_millis()) else {
            return;
        };
        let shared = Arc::downgrade(&self.state);
        let key = (session_id.clone(), review_at);
        let task = tokio::spawn(async move {
            loop {
                tokio::time::sleep(next_render_delay(deadline, now_millis())).await;
                let Some(shared) = shared.upgrade() else {
                    return;
                };
                let binding = shared
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .bindings
                    .get(&key)
                    .map(|binding| binding.requests.clone());
                let rendered = request_scoped_render(binding).await;
                if now_millis() >= deadline && rendered != Some(false) {
                    return;
                }
            }
        });
        state.timers.insert(session_id, task);
    }
}

async fn request_scoped_render(binding: Option<mpsc::UnboundedSender<RenderRequest>>) -> Option<bool> {
    let binding = binding?;
    let (reply, rendered) = oneshot::channel();
    // Closing a binding is the native equivalent of removing the JS callback.
    binding.send(reply).ok()?;
    rendered.await.ok()
}

fn next_render_delay(deadline: i64, now: i64) -> Duration {
    let remaining = deadline - now;
    let next = if remaining > 0 {
        let seconds = (remaining + 999) / 1000;
        deadline - (seconds - 1) * 1000
    } else {
        now + 1000
    };
    Duration::from_millis((next - now).max(1) as u64)
}

/// `Date.parse(reviewAt)` as epoch milliseconds; `None` for JavaScript `NaN`.
pub(super) fn parse_review_deadline(review_at: &JsString) -> Option<i64> {
    let millis = date_parse_utf16(review_at.units());
    (!millis.is_nan()).then_some(millis as i64)
}

pub fn create_wait_countdown_refresher() -> WaitCountdownRefresher {
    WaitCountdownRefresher::default()
}

fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as i64)
        .unwrap_or(0)
}
