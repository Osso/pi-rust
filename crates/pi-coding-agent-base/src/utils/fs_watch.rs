use std::path::{Path, PathBuf};

use notify::event::ModifyKind;
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};

pub const FS_WATCH_RETRY_DELAY_MS: u64 = 5_000;

/// A file system event kind compatible with `fs.watch` listeners.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WatchEventKind {
    Rename,
    Change,
}

/// Close a watcher, ignoring close errors.
///
/// notify stops watching when the watcher is dropped; dropping is infallible.
pub fn close_watcher(watcher: Option<RecommendedWatcher>) {
    drop(watcher);
}

/// Watch a path and report setup or runtime errors to `on_error`.
pub fn watch_with_error_handler<L, E>(path: &Path, listener: L, on_error: E) -> Option<RecommendedWatcher>
where
    L: Fn(WatchEventKind, Option<PathBuf>) + Send + 'static,
    E: Fn() + Send + Clone + 'static,
{
    let error_handler = on_error.clone();
    let mut watcher = match notify::recommended_watcher(move |result: notify::Result<Event>| match result {
        Ok(event) if matches!(event.kind, EventKind::Access(_)) => {
            // `fs.watch` reports renames and content changes, not reads (atime).
        }
        Ok(event) => listener(watch_event_kind(&event.kind), event.paths.first().cloned()),
        Err(_) => error_handler(),
    }) {
        Ok(watcher) => watcher,
        Err(_) => {
            on_error();
            return None;
        }
    };

    if watcher.watch(path, RecursiveMode::NonRecursive).is_err() {
        on_error();
        return None;
    }

    Some(watcher)
}

fn watch_event_kind(kind: &EventKind) -> WatchEventKind {
    match kind {
        EventKind::Create(_) | EventKind::Remove(_) | EventKind::Modify(ModifyKind::Name(_)) => WatchEventKind::Rename,
        _ => WatchEventKind::Change,
    }
}
