//! Reason-bearing AbortController/AbortSignal semantics shared by every Rust package.

use crate::js_json::JsValue;
use crate::node_error::NativeNodeError;
use crate::number::js_number_to_string;
use indexmap::IndexMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;
use tokio::sync::Notify;
use tokio::task::AbortHandle;

/// Preserve Error/DOMException branding independently from plain JSON lookalikes.
#[derive(Debug, Clone, PartialEq)]
pub enum AbortReason {
    Value(JsValue),
    Error(NativeNodeError),
    DomException(NativeNodeError),
}
impl AbortReason {
    pub fn value(&self) -> JsValue {
        match self {
            Self::Value(value) => value.clone(),
            Self::Error(error) | Self::DomException(error) => error.value(),
        }
    }
    pub fn as_error(&self) -> Option<NativeNodeError> {
        match self {
            Self::Value(_) => None,
            Self::Error(error) | Self::DomException(error) => Some(error.clone()),
        }
    }
}
impl From<JsValue> for AbortReason {
    fn from(value: JsValue) -> Self {
        Self::Value(value)
    }
}
impl From<NativeNodeError> for AbortReason {
    fn from(error: NativeNodeError) -> Self {
        Self::Error(error)
    }
}

#[derive(Clone)]
pub struct AbortSignal {
    inner: Arc<Inner>,
}
impl PartialEq for AbortSignal {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.inner, &other.inner)
    }
}
impl Eq for AbortSignal {}

#[derive(Clone, Default, PartialEq, Eq)]
pub struct AbortController {
    signal: AbortSignal,
}
impl AbortController {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn signal(&self) -> AbortSignal {
        self.signal.clone()
    }
    /// None is JavaScript's omitted/undefined reason, not null or another falsey value.
    pub fn abort(&self, reason: Option<AbortReason>) {
        abort_inner(&self.signal.inner, reason.unwrap_or_else(default_abort_reason));
    }
}
impl std::fmt::Debug for AbortController {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.signal.fmt(f)
    }
}
impl Default for AbortSignal {
    fn default() -> Self {
        Self {
            inner: Arc::new(Inner::default()),
        }
    }
}
impl std::fmt::Debug for AbortSignal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AbortSignal").field("reason", &self.reason()).finish()
    }
}
impl AbortSignal {
    pub fn aborted(&self) -> bool {
        self.reason().is_some()
    }
    /// None is undefined before abort. Returned values follow the approved owned-value rule.
    pub fn reason(&self) -> Option<AbortReason> {
        self.inner.state.lock().expect("abort state lock").reason.clone()
    }
    pub async fn wait(&self) -> AbortReason {
        loop {
            let notified = self.inner.changed.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if let Some(reason) = self.reason() {
                return reason;
            }
            notified.await;
        }
    }
    /// One-shot native listener. Already-aborted signals return the reason without replaying it.
    pub fn subscribe<F>(&self, callback: F) -> Result<AbortSubscription, AbortReason>
    where
        F: Fn(&AbortReason) + Send + Sync + 'static,
    {
        let mut state = self.inner.state.lock().expect("abort state lock");
        if let Some(reason) = &state.reason {
            return Err(reason.clone());
        }
        let (id, listener) = register_listener(&mut state, callback);
        Ok(AbortSubscription {
            source: self.inner.clone(),
            listener,
            id,
        })
    }
    /// Retain a one-shot listener until abort, explicit removal, or signal destruction.
    /// Dropping the returned identity does not remove it; already-aborted signals do not replay.
    pub fn add_listener<F>(&self, callback: F) -> ListenerId
    where
        F: Fn(&AbortReason) + Send + Sync + 'static,
    {
        let mut state = self.inner.state.lock().expect("abort state lock");
        let (id, listener) = register_listener(&mut state, callback);
        ListenerId {
            source: Arc::downgrade(&self.inner),
            listener: Arc::downgrade(&listener),
            id,
        }
    }

    /// Remove only this signal's listener, including while its abort event is being dispatched.
    pub fn remove_listener(&self, id: &ListenerId) {
        if !id.source.ptr_eq(&Arc::downgrade(&self.inner)) {
            return;
        }
        if let Some(listener) = id.listener.upgrade() {
            listener.active.store(false, Ordering::Release);
        }
        self.inner
            .state
            .lock()
            .expect("abort state lock")
            .listeners
            .shift_remove(&id.id);
    }

    pub fn abort(reason: Option<AbortReason>) -> Self {
        let controller = AbortController::new();
        controller.abort(reason);
        controller.signal()
    }
    pub fn any(signals: &[Self]) -> Self {
        let signal = Self::default();
        for source in signals {
            match subscribe_forwarded_abort(source, &signal.inner) {
                Ok(subscription) => attach_dependency(&signal.inner, subscription),
                Err(reason) => {
                    abort_inner(&signal.inner, reason);
                    break;
                }
            }
            if signal.aborted() {
                break;
            }
        }
        signal
    }
    /// Like Node's unref'ed timer, this task does not keep its runtime or an unowned signal alive.
    pub fn timeout(delay_ms: f64) -> Result<Self, NativeNodeError> {
        let delay = validate_timeout(delay_ms)?;
        let signal = Self::default();
        let target = Arc::downgrade(&signal.inner);
        let timer = tokio::spawn(async move {
            tokio::time::sleep(delay).await;
            if let Some(target) = target.upgrade() {
                abort_inner(&target, timeout_reason());
            }
        });
        *signal.inner.timer.lock().expect("abort timer lock") = Some(timer.abort_handle());
        Ok(signal)
    }
}

type AbortListener = dyn Fn(&AbortReason) + Send + Sync;
struct Listener {
    active: AtomicBool,
    callback: Box<AbortListener>,
}
#[derive(Default)]
struct State {
    reason: Option<AbortReason>,
    next_listener: u64,
    listeners: IndexMap<u64, Arc<Listener>>,
    dependencies: Vec<AbortSubscription>,
}
#[derive(Default)]
struct Inner {
    state: Mutex<State>,
    changed: Notify,
    timer: Mutex<Option<AbortHandle>>,
}
impl Drop for Inner {
    fn drop(&mut self) {
        if let Some(timer) = self.timer.get_mut().expect("abort timer lock").take() {
            timer.abort();
        }
    }
}

/// Opaque listener identity. Cloning/dropping it neither retains the signal nor detaches its listener.
#[derive(Clone)]
pub struct ListenerId {
    source: Weak<Inner>,
    listener: Weak<Listener>,
    id: u64,
}

fn register_listener<F>(state: &mut State, callback: F) -> (u64, Arc<Listener>)
where
    F: Fn(&AbortReason) + Send + Sync + 'static,
{
    let id = state.next_listener;
    state.next_listener += 1;
    let listener = Arc::new(Listener {
        active: AtomicBool::new(true),
        callback: Box::new(callback),
    });
    state.listeners.insert(id, listener.clone());
    (id, listener)
}

/// Dropping this handle removes the listener, including during an in-progress dispatch.
pub struct AbortSubscription {
    source: Arc<Inner>,
    listener: Arc<Listener>,
    id: u64,
}
impl Drop for AbortSubscription {
    fn drop(&mut self) {
        self.listener.active.store(false, Ordering::Release);
        self.source
            .state
            .lock()
            .expect("abort state lock")
            .listeners
            .shift_remove(&self.id);
    }
}
fn subscribe_forwarded_abort(source: &AbortSignal, target: &Arc<Inner>) -> Result<AbortSubscription, AbortReason> {
    let target = Arc::downgrade(target);
    source.subscribe(move |reason| {
        if let Some(target) = target.upgrade() {
            abort_inner(&target, reason.clone());
        }
    })
}
fn attach_dependency(target: &Arc<Inner>, subscription: AbortSubscription) {
    let mut state = target.state.lock().expect("abort state lock");
    if state.reason.is_none() {
        state.dependencies.push(subscription);
        return;
    }
    drop(state);
    drop(subscription);
}
fn abort_inner(inner: &Arc<Inner>, reason: AbortReason) {
    let (listeners, dependencies) = {
        let mut state = inner.state.lock().expect("abort state lock");
        if state.reason.is_some() {
            return;
        }
        state.reason = Some(reason.clone());
        (
            std::mem::take(&mut state.listeners),
            std::mem::take(&mut state.dependencies),
        )
    };
    drop(dependencies);
    for listener in listeners.into_values() {
        if listener.active.swap(false, Ordering::AcqRel) {
            (listener.callback)(&reason);
        }
    }
    inner.changed.notify_waiters();
}
fn default_abort_reason() -> AbortReason {
    AbortReason::DomException(
        NativeNodeError::new("AbortError", "This operation was aborted").with_property("code", JsValue::Number(20.0)),
    )
}
fn timeout_reason() -> AbortReason {
    AbortReason::DomException(
        NativeNodeError::new("TimeoutError", "The operation was aborted due to timeout")
            .with_property("code", JsValue::Number(23.0)),
    )
}
fn validate_timeout(delay: f64) -> Result<Duration, NativeNodeError> {
    let constraint = if !delay.is_finite() || delay.fract() != 0.0 {
        "an integer"
    } else if !(0.0..=f64::from(u32::MAX)).contains(&delay) {
        ">= 0 && <= 4294967295"
    } else {
        let normalized = crate::node_timers::normalize_timeout_delay(delay);
        return Ok(Duration::from_millis(normalized as u64));
    };
    let received = format_range_number(delay);
    Err(NativeNodeError::new(
        "RangeError",
        format!("The value of \"delay\" is out of range. It must be {constraint}. Received {received}"),
    )
    .with_property("code", JsValue::String("ERR_OUT_OF_RANGE".into())))
}
fn format_range_number(value: f64) -> String {
    let text = js_number_to_string(value);
    if !value.is_finite() || value.fract() != 0.0 || value.abs() <= 4_294_967_296.0 {
        return text;
    }
    let (sign, digits) = text
        .strip_prefix('-')
        .map_or(("", text.as_str()), |digits| ("-", digits));
    // Node's ERR_OUT_OF_RANGE groups the rendered text, even exponent characters.
    let chunks = digits.as_bytes().rchunks(3).rev();
    let groups: Vec<_> = chunks
        .map(|chunk| std::str::from_utf8(chunk).expect("ASCII number rendering"))
        .collect();
    format!("{sign}{}", groups.join("_"))
}
