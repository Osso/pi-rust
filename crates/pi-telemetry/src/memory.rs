use std::sync::{Arc, Mutex, MutexGuard};

use serde::{Deserialize, Serialize};

use crate::{
    BoxFuture, ErasedSpanCallback, SpanAttributes, SpanError, SpanOptions, SpanOutcome, SpanStatus, TelemetryContext,
    TelemetrySpan,
};

/// One recorded span event.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordedTelemetryEvent {
    pub name: String,
    pub attributes: SpanAttributes,
}

/// Detached snapshot of one recorded span.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordedTelemetrySpan {
    pub id: u64,
    pub parent_id: Option<u64>,
    pub name: String,
    pub attributes: SpanAttributes,
    pub events: Vec<RecordedTelemetryEvent>,
    pub status: SpanStatus,
    pub settled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_sequence: Option<u64>,
}

struct MutableRecordedSpan {
    id: u64,
    parent_id: Option<u64>,
    name: String,
    attributes: SpanAttributes,
    events: Vec<RecordedTelemetryEvent>,
    status: SpanStatus,
    explicit_status: bool,
    settled: bool,
    end_sequence: Option<u64>,
}

pub(crate) struct InMemoryState {
    spans: Vec<Arc<Mutex<MutableRecordedSpan>>>,
    next_span_id: u64,
    next_end_sequence: u64,
}

#[derive(Clone)]
pub(crate) struct InMemorySpanHandle {
    pub(crate) state: Arc<Mutex<InMemoryState>>,
    recorded: Arc<Mutex<MutableRecordedSpan>>,
}

impl InMemorySpanHandle {
    fn is_settled(&self) -> bool {
        lock(&self.recorded).settled
    }

    fn id(&self) -> u64 {
        lock(&self.recorded).id
    }

    pub(crate) fn add_event(&self, name: String, attributes: Option<SpanAttributes>) {
        let mut span = lock(&self.recorded);
        if span.settled {
            return;
        }
        span.events.push(RecordedTelemetryEvent {
            name,
            attributes: copy_attributes(attributes.as_ref()),
        });
    }

    pub(crate) fn set_attributes(&self, attributes: SpanAttributes) {
        let mut span = lock(&self.recorded);
        if span.settled {
            return;
        }
        span.attributes.extend(attributes);
    }

    pub(crate) fn set_status(&self, status: SpanStatus) {
        let mut span = lock(&self.recorded);
        if span.settled {
            return;
        }
        span.status = status;
        span.explicit_status = true;
    }
}

/// Backend-neutral reference implementation that records spans in process memory.
#[derive(Clone)]
pub struct InMemoryTelemetryContext {
    state: Arc<Mutex<InMemoryState>>,
}

impl Default for InMemoryTelemetryContext {
    fn default() -> Self {
        Self {
            state: Arc::new(Mutex::new(InMemoryState {
                spans: Vec::new(),
                next_span_id: 1,
                next_end_sequence: 1,
            })),
        }
    }
}

impl InMemoryTelemetryContext {
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns detached snapshots in span-start order.
    pub fn get_spans(&self) -> Vec<RecordedTelemetrySpan> {
        let spans = lock(&self.state).spans.clone();
        spans.iter().map(|span| snapshot(span)).collect()
    }
}

impl TelemetryContext for InMemoryTelemetryContext {
    fn start_span_erased<'a>(
        &'a self,
        options: SpanOptions,
        callback: ErasedSpanCallback<'a>,
    ) -> BoxFuture<'a, SpanOutcome> {
        start_in_memory_span(&self.state, None, options, callback)
    }
}

pub(crate) fn start_in_memory_span<'a>(
    state: &Arc<Mutex<InMemoryState>>,
    parent: Option<&InMemorySpanHandle>,
    options: SpanOptions,
    callback: ErasedSpanCallback<'a>,
) -> BoxFuture<'a, SpanOutcome> {
    if parent.is_some_and(InMemorySpanHandle::is_settled) {
        return callback(TelemetrySpan::noop());
    }

    let handle = create_span(state, parent, options);
    let span = TelemetrySpan::in_memory(handle.clone());
    let outcome = callback(span);
    let state = Arc::clone(state);

    Box::pin(async move {
        let outcome = outcome.await;
        settle_span(&state, &handle.recorded, outcome.error());
        outcome
    })
}

fn create_span(
    state: &Arc<Mutex<InMemoryState>>,
    parent: Option<&InMemorySpanHandle>,
    options: SpanOptions,
) -> InMemorySpanHandle {
    let mut state_guard = lock(state);
    let id = state_guard.next_span_id;
    state_guard.next_span_id += 1;
    let parent_id = parent.map(InMemorySpanHandle::id);
    let recorded = Arc::new(Mutex::new(MutableRecordedSpan {
        id,
        parent_id,
        name: options.name,
        attributes: copy_attributes(options.attributes.as_ref()),
        events: Vec::new(),
        status: SpanStatus::Ok,
        explicit_status: false,
        settled: false,
        end_sequence: None,
    }));
    state_guard.spans.push(Arc::clone(&recorded));
    drop(state_guard);

    InMemorySpanHandle {
        state: Arc::clone(state),
        recorded,
    }
}

fn settle_span(state: &Mutex<InMemoryState>, recorded: &Mutex<MutableRecordedSpan>, error: Option<&SpanError>) {
    let mut state = lock(state);
    let mut span = lock(recorded);
    if span.settled {
        return;
    }
    if let Some(error) = error
        && !span.explicit_status
    {
        span.status = SpanStatus::Error {
            error: Some(error.clone()),
        };
    }
    span.settled = true;
    span.end_sequence = Some(state.next_end_sequence);
    state.next_end_sequence += 1;
}

fn snapshot(recorded: &Mutex<MutableRecordedSpan>) -> RecordedTelemetrySpan {
    let span = lock(recorded);
    RecordedTelemetrySpan {
        id: span.id,
        parent_id: span.parent_id,
        name: span.name.clone(),
        attributes: span.attributes.clone(),
        events: span.events.clone(),
        status: span.status.clone(),
        settled: span.settled,
        end_sequence: span.end_sequence,
    }
}

fn copy_attributes(attributes: Option<&SpanAttributes>) -> SpanAttributes {
    attributes.cloned().unwrap_or_default()
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().expect("telemetry state lock must not be poisoned")
}
