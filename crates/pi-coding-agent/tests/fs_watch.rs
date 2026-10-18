use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, channel};
use std::time::Duration;

use pi_coding_agent::utils::fs_watch::{
    FS_WATCH_RETRY_DELAY_MS, WatchEventKind, close_watcher, watch_with_error_handler,
};
use tempfile::TempDir;

fn wait_for(
    receiver: &Receiver<(WatchEventKind, Option<std::path::PathBuf>)>,
    expected: WatchEventKind,
) -> Option<std::path::PathBuf> {
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while std::time::Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        let event = receiver.recv_timeout(remaining).expect("watch event");
        if event.0 == expected {
            return event.1;
        }
    }
    None
}

#[test]
fn reports_creation_and_modification_events() {
    let dir = TempDir::new().expect("temp dir");
    let (sender, receiver) = channel();
    let watcher = watch_with_error_handler(
        dir.path(),
        move |kind, path| {
            let _ = sender.send((kind, path));
        },
        || panic!("watch must not fail"),
    )
    .expect("watcher");

    let file = dir.path().join("new.txt");
    std::fs::write(&file, "first").expect("create file");
    let created_path = wait_for(&receiver, WatchEventKind::Rename).expect("creation event");
    assert!(created_path.ends_with("new.txt"), "{}", created_path.display());

    std::fs::write(&file, "second").expect("modify file");
    wait_for(&receiver, WatchEventKind::Change).expect("modification event");

    close_watcher(Some(watcher));
}

#[test]
fn reports_watch_setup_failures_and_returns_none() {
    let dir = TempDir::new().expect("temp dir");
    let missing = dir.path().join("missing");
    let errors = Arc::new(AtomicUsize::new(0));
    let observed = errors.clone();

    let watcher = watch_with_error_handler(
        &missing,
        |_, _| {},
        move || {
            observed.fetch_add(1, Ordering::SeqCst);
        },
    );

    assert!(watcher.is_none());
    assert_eq!(errors.load(Ordering::SeqCst), 1);
}

#[test]
fn closing_a_missing_watcher_is_a_no_op() {
    close_watcher(None);
}

#[test]
fn retry_delay_matches_the_source_constant() {
    assert_eq!(FS_WATCH_RETRY_DELAY_MS, 5_000);
}
