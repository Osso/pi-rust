use std::any::Any;
use std::collections::{HashMap, HashSet};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{Arc, Mutex};

use pi_protocol::{CommandResult, ServerEvent, ServerSnapshot, SessionSnapshot};

use crate::errors::PiError;
use crate::types::{EventListener, ListenerErrorHandler, SessionSnapshotListener, SnapshotListener, Unsubscribe};

type SnapshotEntry = (u64, SnapshotListener);
type EventEntry = (u64, EventListener);
type SessionSnapshotEntry = (u64, SessionSnapshotListener);
type SessionEventEntry = (u64, EventListener);

#[derive(Default)]
struct StateInner {
    snapshot: Option<ServerSnapshot>,
    session_snapshots: HashMap<String, SessionSnapshot>,
    attached_session_ids: HashSet<String>,
    snapshot_listeners: Vec<SnapshotEntry>,
    event_listeners: Vec<EventEntry>,
    session_snapshot_listeners: HashMap<String, Vec<SessionSnapshotEntry>>,
    session_event_listeners: HashMap<String, Vec<SessionEventEntry>>,
    next_listener_id: u64,
}

/// Authoritative client-side snapshot and event reduction.
#[derive(Clone)]
pub(crate) struct ClientState {
    inner: Arc<Mutex<StateInner>>,
    on_listener_error: Option<ListenerErrorHandler>,
}

impl ClientState {
    pub(crate) fn new(on_listener_error: Option<ListenerErrorHandler>) -> Self {
        Self {
            inner: Arc::new(Mutex::new(StateInner::default())),
            on_listener_error,
        }
    }

    pub(crate) fn snapshot(&self) -> Option<ServerSnapshot> {
        self.inner.lock().unwrap().snapshot.clone()
    }

    pub(crate) fn reset(&self) {
        let mut inner = self.inner.lock().unwrap();
        inner.snapshot = None;
        inner.session_snapshots.clear();
        inner.attached_session_ids.clear();
    }

    pub(crate) fn clear_attachments(&self) {
        self.inner.lock().unwrap().attached_session_ids.clear();
    }

    pub(crate) fn dispose(&self) {
        let mut inner = self.inner.lock().unwrap();
        inner.snapshot = None;
        inner.session_snapshots.clear();
        inner.attached_session_ids.clear();
        inner.snapshot_listeners.clear();
        inner.event_listeners.clear();
        inner.session_snapshot_listeners.clear();
        inner.session_event_listeners.clear();
    }

    pub(crate) fn get_session_snapshot(&self, session_id: &str) -> Option<SessionSnapshot> {
        self.inner.lock().unwrap().session_snapshots.get(session_id).cloned()
    }

    pub(crate) fn is_session_attached(&self, session_id: &str) -> bool {
        self.inner.lock().unwrap().attached_session_ids.contains(session_id)
    }

    pub(crate) fn forget_session_snapshot(&self, session_id: &str) -> Option<SessionSnapshot> {
        self.inner.lock().unwrap().session_snapshots.remove(session_id)
    }

    pub(crate) fn restore_session_snapshot(&self, snapshot: SessionSnapshot) {
        self.inner
            .lock()
            .unwrap()
            .session_snapshots
            .entry(snapshot.id().to_string())
            .or_insert(snapshot);
    }

    pub(crate) fn subscribe(&self, listener: SnapshotListener) -> Unsubscribe {
        let id = {
            let mut inner = self.inner.lock().unwrap();
            inner.next_listener_id += 1;
            let id = inner.next_listener_id;
            inner.snapshot_listeners.push((id, listener));
            id
        };
        remove_on_unsubscribe(self.inner.clone(), move |inner| {
            inner.snapshot_listeners.retain(|(entry_id, _)| *entry_id != id);
        })
    }

    pub(crate) fn on_event(&self, listener: EventListener) -> Unsubscribe {
        let id = {
            let mut inner = self.inner.lock().unwrap();
            inner.next_listener_id += 1;
            let id = inner.next_listener_id;
            inner.event_listeners.push((id, listener));
            id
        };
        remove_on_unsubscribe(self.inner.clone(), move |inner| {
            inner.event_listeners.retain(|(entry_id, _)| *entry_id != id);
        })
    }

    pub(crate) fn subscribe_session(&self, session_id: &str, listener: SessionSnapshotListener) -> Unsubscribe {
        let id = {
            let mut inner = self.inner.lock().unwrap();
            inner.next_listener_id += 1;
            let id = inner.next_listener_id;
            inner
                .session_snapshot_listeners
                .entry(session_id.to_string())
                .or_default()
                .push((id, listener));
            id
        };
        let session_id = session_id.to_string();
        remove_on_unsubscribe(self.inner.clone(), move |inner| {
            if let Some(listeners) = inner.session_snapshot_listeners.get_mut(&session_id) {
                listeners.retain(|(entry_id, _)| *entry_id != id);
                if listeners.is_empty() {
                    inner.session_snapshot_listeners.remove(&session_id);
                }
            }
        })
    }

    pub(crate) fn on_session_event(&self, session_id: &str, listener: EventListener) -> Unsubscribe {
        let id = {
            let mut inner = self.inner.lock().unwrap();
            inner.next_listener_id += 1;
            let id = inner.next_listener_id;
            inner
                .session_event_listeners
                .entry(session_id.to_string())
                .or_default()
                .push((id, listener));
            id
        };
        let session_id = session_id.to_string();
        remove_on_unsubscribe(self.inner.clone(), move |inner| {
            if let Some(listeners) = inner.session_event_listeners.get_mut(&session_id) {
                listeners.retain(|(entry_id, _)| *entry_id != id);
                if listeners.is_empty() {
                    inner.session_event_listeners.remove(&session_id);
                }
            }
        })
    }

    pub(crate) fn apply_result(&self, result: &CommandResult) {
        let (listeners, snapshot) = match result {
            CommandResult::List(_) => return,
            CommandResult::Detach(result) => {
                let session_id = result.session_id();
                let mut inner = self.inner.lock().unwrap();
                inner.attached_session_ids.remove(session_id);
                let snapshot = inner.session_snapshots.get(session_id).cloned();
                match snapshot {
                    Some(snapshot) => {
                        let detached = clone_snapshot_with_attached(&snapshot, false);
                        let listeners = store_session_snapshot(&mut inner, &detached, true);
                        (listeners, Some(detached))
                    }
                    None => (Vec::new(), None),
                }
            }
            result => match command_result_snapshot(result) {
                Some(snapshot) => {
                    let mut inner = self.inner.lock().unwrap();
                    let listeners = store_session_snapshot(&mut inner, snapshot, false);
                    (listeners, Some(snapshot.clone()))
                }
                None => (Vec::new(), None),
            },
        };
        if let Some(snapshot) = snapshot {
            notify_session_snapshots(listeners, &snapshot, self.on_listener_error.as_ref());
        }
    }

    pub(crate) fn apply_event(&self, event: &ServerEvent) {
        let mut server_snapshot = None;
        let mut session_snapshot = None;
        let (event_listeners, session_event_listeners) = {
            let mut inner = self.inner.lock().unwrap();
            match event {
                ServerEvent::ServerSnapshot(snapshot) => {
                    let accepted = inner
                        .snapshot
                        .as_ref()
                        .is_none_or(|current| snapshot.revision() >= current.revision());
                    if accepted {
                        inner.snapshot = Some(snapshot.clone());
                        server_snapshot = Some((
                            inner
                                .snapshot_listeners
                                .iter()
                                .map(|(_, listener)| listener.clone())
                                .collect::<Vec<_>>(),
                            snapshot.clone(),
                        ));
                    }
                }
                ServerEvent::SessionSnapshot(snapshot) => {
                    let listeners = store_session_snapshot(&mut inner, snapshot, false);
                    let accepted = inner
                        .session_snapshots
                        .get(snapshot.id())
                        .is_some_and(|stored| stored.revision() == snapshot.revision());
                    if accepted {
                        session_snapshot = Some((listeners, snapshot.clone()));
                    }
                }
                ServerEvent::SessionRemoved(event) => {
                    inner.session_snapshots.remove(event.session_id());
                    inner.attached_session_ids.remove(event.session_id());
                }
                ServerEvent::SessionProgress(_) => {}
            }
            let event_listeners = collect_event_listeners(&inner.event_listeners);
            let session_event_listeners = event_session_id(event)
                .and_then(|session_id| inner.session_event_listeners.get(session_id))
                .map(|listeners| collect_event_listeners(listeners))
                .unwrap_or_default();
            (event_listeners, session_event_listeners)
        };
        if let Some((listeners, snapshot)) = server_snapshot {
            notify_snapshots(listeners, &snapshot, self.on_listener_error.as_ref());
        }
        if let Some((listeners, snapshot)) = session_snapshot {
            notify_session_snapshots(listeners, &snapshot, self.on_listener_error.as_ref());
        }
        notify_events(event_listeners, event, self.on_listener_error.as_ref());
        notify_events(session_event_listeners, event, self.on_listener_error.as_ref());
    }

    pub(crate) fn apply_server_snapshot(&self, snapshot: &ServerSnapshot) {
        let listeners = {
            let mut inner = self.inner.lock().unwrap();
            if let Some(current) = &inner.snapshot
                && snapshot.revision() < current.revision()
            {
                return;
            }
            inner.snapshot = Some(snapshot.clone());
            inner
                .snapshot_listeners
                .iter()
                .map(|(_, listener)| listener.clone())
                .collect::<Vec<_>>()
        };
        notify_snapshots(listeners, snapshot, self.on_listener_error.as_ref());
    }
}

fn clone_snapshot_with_attached(snapshot: &SessionSnapshot, attached: bool) -> SessionSnapshot {
    SessionSnapshot::new(pi_protocol::SessionSnapshotInput {
        id: snapshot.id().to_string(),
        name: snapshot.name().map(str::to_string),
        cwd: snapshot.cwd().to_string(),
        created_at: snapshot.created_at(),
        updated_at: snapshot.updated_at(),
        phase: snapshot.phase(),
        model: snapshot.model().clone(),
        thinking_level: snapshot.thinking_level(),
        attached,
        locked: snapshot.locked(),
        revision: snapshot.revision(),
        transcript: snapshot.transcript().to_vec(),
        queued_steer: snapshot.queued_steer().to_vec(),
        queued_steer_count: snapshot.queued_steer_count(),
    })
    .expect("existing session snapshot fields remain valid")
}

fn command_result_snapshot(result: &CommandResult) -> Option<&SessionSnapshot> {
    match result {
        CommandResult::Create(snapshot)
        | CommandResult::Attach(snapshot)
        | CommandResult::Prompt(snapshot)
        | CommandResult::Steer(snapshot)
        | CommandResult::Abort(snapshot)
        | CommandResult::SetModel(snapshot)
        | CommandResult::SetThinking(snapshot) => Some(snapshot),
        CommandResult::List(_) | CommandResult::Detach(_) => None,
    }
}

fn store_session_snapshot(
    inner: &mut StateInner,
    snapshot: &SessionSnapshot,
    force: bool,
) -> Vec<SessionSnapshotListener> {
    let session_id = snapshot.id().to_string();
    if let Some(current) = inner.session_snapshots.get(&session_id)
        && !force
        && snapshot.revision() < current.revision()
    {
        return Vec::new();
    }
    inner.session_snapshots.insert(session_id.clone(), snapshot.clone());
    if snapshot.attached() {
        inner.attached_session_ids.insert(session_id.clone());
    } else {
        inner.attached_session_ids.remove(&session_id);
    }
    inner
        .session_snapshot_listeners
        .get(&session_id)
        .map(|listeners| listeners.iter().map(|(_, listener)| listener.clone()).collect())
        .unwrap_or_default()
}

fn event_session_id(event: &ServerEvent) -> Option<&str> {
    match event {
        ServerEvent::SessionSnapshot(snapshot) => Some(snapshot.id()),
        ServerEvent::SessionProgress(progress) => Some(progress.session_id()),
        ServerEvent::SessionRemoved(removed) => Some(removed.session_id()),
        ServerEvent::ServerSnapshot(_) => None,
    }
}

fn collect_event_listeners(listeners: &[EventEntry]) -> Vec<EventListener> {
    listeners.iter().map(|(_, listener)| listener.clone()).collect()
}

fn remove_on_unsubscribe<F>(inner: Arc<Mutex<StateInner>>, remove: F) -> Unsubscribe
where
    F: Fn(&mut StateInner) + Send + Sync + 'static,
{
    Box::new(move || {
        let mut inner = inner.lock().unwrap();
        remove(&mut inner);
    })
}

fn notify_snapshots(
    listeners: Vec<SnapshotListener>,
    value: &ServerSnapshot,
    on_listener_error: Option<&ListenerErrorHandler>,
) {
    for listener in listeners {
        let result = catch_unwind(AssertUnwindSafe(|| listener(value)));
        if let Err(payload) = result {
            report_listener_error(on_listener_error, payload);
        }
    }
}

fn notify_session_snapshots(
    listeners: Vec<SessionSnapshotListener>,
    value: &SessionSnapshot,
    on_listener_error: Option<&ListenerErrorHandler>,
) {
    for listener in listeners {
        let result = catch_unwind(AssertUnwindSafe(|| listener(value)));
        if let Err(payload) = result {
            report_listener_error(on_listener_error, payload);
        }
    }
}

fn notify_events(listeners: Vec<EventListener>, value: &ServerEvent, on_listener_error: Option<&ListenerErrorHandler>) {
    for listener in listeners {
        let result = catch_unwind(AssertUnwindSafe(|| listener(value)));
        if let Err(payload) = result {
            report_listener_error(on_listener_error, payload);
        }
    }
}

fn report_listener_error(on_listener_error: Option<&ListenerErrorHandler>, payload: Box<dyn Any + Send>) {
    let Some(handler) = on_listener_error else {
        return;
    };
    let message = if let Some(message) = payload.downcast_ref::<&str>() {
        (*message).to_string()
    } else if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else {
        "listener panicked".to_string()
    };
    let _ = catch_unwind(AssertUnwindSafe(|| handler(PiError::Listener(message))));
}
