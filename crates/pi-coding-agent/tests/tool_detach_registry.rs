use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use pi_coding_agent::core::tool_detach_registry::{DEFAULT_AUTO_DETACH_AFTER_MS, ToolDetachHandle, ToolDetachRegistry};

struct RecordingHandle {
    name: &'static str,
    detached: AtomicBool,
    succeed: bool,
    log: Arc<Mutex<Vec<&'static str>>>,
}

impl RecordingHandle {
    fn new(name: &'static str, succeed: bool, log: &Arc<Mutex<Vec<&'static str>>>) -> Arc<Self> {
        Arc::new(Self {
            name,
            detached: AtomicBool::new(false),
            succeed,
            log: Arc::clone(log),
        })
    }
}

impl ToolDetachHandle for RecordingHandle {
    fn detach(&self) -> Result<bool, pi_js_compat::callback_error::CallbackError> {
        self.detached.store(true, Ordering::SeqCst);
        self.log.lock().unwrap().push(self.name);
        Ok(self.succeed)
    }
}

// Frozen tool-detach-registry.ts:35-37: snapshot all, preserve insertion order,
// and never stop after a failed or successful detach result.
#[test]
fn detaches_every_handle_in_order_without_unregistering_it() {
    let registry = Arc::new(ToolDetachRegistry::new());
    let log = Arc::new(Mutex::new(Vec::new()));
    let subscriptions: Vec<_> = [("first", false), ("second", true), ("third", true)]
        .into_iter()
        .map(|(name, succeed)| registry.register(RecordingHandle::new(name, succeed, &log)))
        .collect();
    registry.detach_all().expect("fixture detach callback");
    assert_eq!(*log.lock().unwrap(), vec!["first", "second", "third"]);
    assert!(registry.has_running());
    drop(subscriptions);
    registry.detach_all().expect("fixture detach callback");
    assert_eq!(*log.lock().unwrap(), vec!["first", "second", "third"]);
}

#[test]
fn default_auto_detach_matches_the_source() {
    assert_eq!(DEFAULT_AUTO_DETACH_AFTER_MS, 600_000);
}

#[test]
fn reports_running_handles_and_unregisters_them() {
    let registry = Arc::new(ToolDetachRegistry::new());
    let log = Arc::new(Mutex::new(Vec::new()));
    let handle = RecordingHandle::new("a", true, &log);

    assert!(!registry.has_running());
    let subscription = registry.register(handle.clone());
    assert!(registry.has_running());

    drop(subscription);
    assert!(!registry.has_running());
    assert!(!handle.detached.load(Ordering::SeqCst));
}

#[test]
fn detaches_the_most_recently_registered_running_handle() {
    let registry = Arc::new(ToolDetachRegistry::new());
    let log = Arc::new(Mutex::new(Vec::new()));
    let first = RecordingHandle::new("first", false, &log);
    let second = RecordingHandle::new("second", true, &log);
    let third = RecordingHandle::new("third", true, &log);
    let _first = registry.register(first.clone());
    let _second = registry.register(second.clone());
    let _third = registry.register(third.clone());

    assert!(registry.detach_running().expect("fixture detach callback"));

    assert_eq!(*log.lock().unwrap(), vec!["third"]);
    assert!(!first.detached.load(Ordering::SeqCst));
    assert!(!second.detached.load(Ordering::SeqCst));
    assert!(third.detached.load(Ordering::SeqCst));
}

#[test]
fn detach_running_reports_when_no_handle_accepts_detachment() {
    let registry = Arc::new(ToolDetachRegistry::new());
    let log = Arc::new(Mutex::new(Vec::new()));
    let _first = registry.register(RecordingHandle::new("first", false, &log));
    let _second = registry.register(RecordingHandle::new("second", false, &log));

    assert!(!registry.detach_running().expect("fixture detach callback"));
    assert_eq!(*log.lock().unwrap(), vec!["second", "first"]);
}

#[test]
fn detach_running_without_handles_is_false() {
    let registry = Arc::new(ToolDetachRegistry::new());
    assert!(!registry.detach_running().expect("fixture detach callback"));
}

#[tokio::test]
async fn auto_detaches_still_registered_handles_after_the_timeout() {
    let registry = Arc::new(ToolDetachRegistry::with_auto_detach_after_ms(20.0));
    let log = Arc::new(Mutex::new(Vec::new()));
    let handle = RecordingHandle::new("timed", true, &log);
    let _subscription = registry.register(handle.clone());

    tokio::time::sleep(Duration::from_millis(80)).await;

    assert!(handle.detached.load(Ordering::SeqCst));
    assert_eq!(*log.lock().unwrap(), vec!["timed"]);
}

#[tokio::test]
async fn auto_detach_skips_handles_unregistered_before_the_timeout() {
    let registry = Arc::new(ToolDetachRegistry::with_auto_detach_after_ms(20.0));
    let log = Arc::new(Mutex::new(Vec::new()));
    let handle = RecordingHandle::new("released", true, &log);
    let subscription = registry.register(handle.clone());

    drop(subscription);
    tokio::time::sleep(Duration::from_millis(80)).await;

    assert!(!handle.detached.load(Ordering::SeqCst));
    assert!(log.lock().unwrap().is_empty());
}

#[tokio::test]
async fn every_registration_gets_its_own_timer() {
    let registry = Arc::new(ToolDetachRegistry::with_auto_detach_after_ms(20.0));
    let log = Arc::new(Mutex::new(Vec::new()));
    let first = RecordingHandle::new("first", true, &log);
    let second = RecordingHandle::new("second", true, &log);
    let _first = registry.register(first.clone());
    let _second = registry.register(second.clone());

    tokio::time::sleep(Duration::from_millis(80)).await;

    assert!(first.detached.load(Ordering::SeqCst));
    assert!(second.detached.load(Ordering::SeqCst));
}
