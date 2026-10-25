//! Port of `packages/ai/src/utils/event-stream.ts`.
//!
//! The TypeScript `EventStream` is one object that producers push into and a
//! consumer iterates. Rust splits it into an [`EventSender`] (push/end/fail)
//! and an [`EventStream`] (next/result) sharing one state.

use std::collections::VecDeque;
use std::fmt;
use std::sync::{Arc, Mutex, MutexGuard};

use pi_js_compat::callback_error::CallbackError;
use tokio::sync::Notify;

use crate::types::{AssistantMessage, AssistantMessageEvent};

/// Failure passed to [`EventSender::fail`]; maps the TypeScript `unknown` error value.
#[derive(Debug, Clone, PartialEq)]
pub struct StreamFailure {
    error: CallbackError,
}

impl StreamFailure {
    pub fn new(message: impl Into<pi_js_compat::js_json::JsString>) -> Self {
        Self {
            error: CallbackError::new(message),
        }
    }

    pub fn message(&self) -> &pi_js_compat::js_json::JsString {
        &self.error.message
    }
}

impl From<CallbackError> for StreamFailure {
    fn from(error: CallbackError) -> Self {
        Self { error }
    }
}

impl fmt::Display for StreamFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&pi_js_compat::js_json::encode_node_utf8(self.message()))
    }
}

impl std::error::Error for StreamFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.error)
    }
}

/// Optional async iterator `return()` boundary supplied by a native stream adapter.
/// Invocation can fail synchronously; a returned future can settle independently.
pub type IteratorReturnFuture = crate::request::BoxFuture<'static, Result<(), StreamFailure>>;
pub type IteratorReturnHook = Arc<dyn Fn() -> Result<IteratorReturnFuture, StreamFailure> + Send + Sync>;

type CompletionCheck<T> = Box<dyn Fn(&T) -> bool + Send + Sync>;
type ResultExtractor<T, R> = Box<dyn Fn(&T) -> R + Send + Sync>;

struct State<T, R> {
    queue: VecDeque<T>,
    done: bool,
    failure: Option<StreamFailure>,
    failure_delivered: bool,
    result: Option<Result<R, StreamFailure>>,
}

struct Shared<T, R> {
    state: Mutex<State<T, R>>,
    changed: Notify,
    is_complete: CompletionCheck<T>,
    extract_result: ResultExtractor<T, R>,
}

impl<T, R> Shared<T, R> {
    fn lock(&self) -> MutexGuard<'_, State<T, R>> {
        self.state.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// Producer half: `push`, `end`, and `fail` from the TypeScript class.
pub struct EventSender<T, R> {
    shared: Arc<Shared<T, R>>,
}

impl<T, R> Clone for EventSender<T, R> {
    fn clone(&self) -> Self {
        Self {
            shared: Arc::clone(&self.shared),
        }
    }
}

/// Consumer half: async iteration plus `result()`.
pub struct EventStream<T, R> {
    shared: Arc<Shared<T, R>>,
    iterator_return: Option<IteratorReturnHook>,
}
impl<T, R> EventStream<T, R> {
    pub fn set_iterator_return(&mut self, callback: Option<IteratorReturnHook>) {
        self.iterator_return = callback;
    }
    /// Invoke the iterator-return callback without choosing whether to await it.
    pub fn return_iterator(&self) -> Result<Option<IteratorReturnFuture>, StreamFailure> {
        self.iterator_return.as_ref().map(|callback| callback()).transpose()
    }
}

/// Creates a connected sender/stream pair (`new EventStream(isComplete, extractResult)`).
pub fn event_stream<T, R>(
    is_complete: impl Fn(&T) -> bool + Send + Sync + 'static,
    extract_result: impl Fn(&T) -> R + Send + Sync + 'static,
) -> (EventSender<T, R>, EventStream<T, R>) {
    let shared = Arc::new(Shared {
        state: Mutex::new(State {
            queue: VecDeque::new(),
            done: false,
            failure: None,
            failure_delivered: false,
            result: None,
        }),
        changed: Notify::new(),
        is_complete: Box::new(is_complete),
        extract_result: Box::new(extract_result),
    });
    (
        EventSender {
            shared: Arc::clone(&shared),
        },
        EventStream {
            shared,
            iterator_return: None,
        },
    )
}

impl<T, R> EventSender<T, R> {
    /// Queues `event`; a completion event also settles the final result. Ignored once done.
    pub fn push(&self, event: T) {
        let mut state = self.shared.lock();
        if state.done {
            return;
        }
        if (self.shared.is_complete)(&event) {
            state.done = true;
            state.result = Some(Ok((self.shared.extract_result)(&event)));
        }
        state.queue.push_back(event);
        drop(state);
        self.shared.changed.notify_waiters();
    }

    /// Ends iteration; `Some(result)` settles the final result, `None` leaves it pending.
    pub fn end(&self, result: Option<R>) {
        let mut state = self.shared.lock();
        if state.done {
            return;
        }
        state.done = true;
        if let Some(result) = result {
            state.result = Some(Ok(result));
        }
        drop(state);
        self.shared.changed.notify_waiters();
    }

    /// Fails iteration (after queued events drain) and rejects the final result.
    pub fn fail(&self, failure: StreamFailure) {
        let mut state = self.shared.lock();
        if state.done {
            return;
        }
        state.done = true;
        state.failure = Some(failure.clone());
        state.result = Some(Err(failure));
        drop(state);
        self.shared.changed.notify_waiters();
    }
}

impl<T, R: Clone> EventStream<T, R> {
    /// Next event; `Some(Err)` once when the stream failed, then `None`.
    pub async fn next(&mut self) -> Option<Result<T, StreamFailure>> {
        loop {
            let changed = self.shared.changed.notified();
            if let Some(item) = self.take_ready() {
                return item;
            }
            changed.await;
        }
    }

    /// Resolves to the final result; pending forever if the stream ended without one.
    pub async fn result(&self) -> Result<R, StreamFailure> {
        loop {
            let changed = self.shared.changed.notified();
            if let Some(result) = self.shared.lock().result.clone() {
                return result;
            }
            changed.await;
        }
    }

    fn take_ready(&self) -> Option<Option<Result<T, StreamFailure>>> {
        let mut state = self.shared.lock();
        if let Some(event) = state.queue.pop_front() {
            return Some(Some(Ok(event)));
        }
        if !state.done {
            return None;
        }
        if state.failure_delivered {
            return Some(None);
        }
        state.failure_delivered = true;
        Some(state.failure.clone().map(Err))
    }
}

pub type AssistantMessageEventSender = EventSender<AssistantMessageEvent, AssistantMessage>;
pub type AssistantMessageEventStream = EventStream<AssistantMessageEvent, AssistantMessage>;

/// `createAssistantMessageEventStream()`: completes on `done`/`error` with the carried message.
pub fn create_assistant_message_event_stream() -> (AssistantMessageEventSender, AssistantMessageEventStream) {
    event_stream(
        |event: &AssistantMessageEvent| {
            matches!(
                event,
                AssistantMessageEvent::Done { .. } | AssistantMessageEvent::Error { .. }
            )
        },
        |event: &AssistantMessageEvent| match event {
            AssistantMessageEvent::Done { message, .. } => message.clone(),
            AssistantMessageEvent::Error { error, .. } => error.clone(),
            _ => unreachable!("completion check admits only done and error events"),
        },
    )
}
