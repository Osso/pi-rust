use std::fs;
use std::path::PathBuf;

use pi_coding_agent::utils::clipboard_temp_files::ClipboardTempFileTracker;
use tempfile::TempDir;

fn create_file(dir: &TempDir, name: &str) -> PathBuf {
    let path = dir.path().join(name);
    fs::write(&path, "data").expect("write file");
    path
}

#[test]
fn deletes_tracked_files_referenced_by_submitted_text() {
    let dir = TempDir::new().expect("temp dir");
    let pasted = create_file(&dir, "pi-clipboard-image.png");
    let unrelated = create_file(&dir, "unrelated.png");
    let mut tracker = ClipboardTempFileTracker::new();

    tracker.track(pasted.clone());
    tracker.cleanup_referenced_in(&format!("describe {}", pasted.display()));

    assert!(!pasted.exists());
    assert!(unrelated.exists());
}

#[test]
fn keeps_tracked_files_until_text_references_them() {
    let dir = TempDir::new().expect("temp dir");
    let pasted = create_file(&dir, "pi-clipboard-image.png");
    let mut tracker = ClipboardTempFileTracker::new();

    tracker.track(pasted.clone());
    tracker.cleanup_referenced_in("different prompt");
    assert!(pasted.exists());

    tracker.cleanup_all();
    assert!(!pasted.exists());
}

#[test]
fn cleanup_all_removes_every_tracked_file_and_forgets_them() {
    let dir = TempDir::new().expect("temp dir");
    let first = create_file(&dir, "first.png");
    let second = create_file(&dir, "second.png");
    let mut tracker = ClipboardTempFileTracker::new();

    tracker.track(first.clone());
    tracker.track(second.clone());
    tracker.cleanup_all();

    assert!(!first.exists());
    assert!(!second.exists());

    // A second cleanup is a no-op and does not panic on missing files.
    tracker.cleanup_all();
}

#[test]
fn tolerates_files_that_already_disappeared() {
    let dir = TempDir::new().expect("temp dir");
    let missing = dir.path().join("missing.png");
    let mut tracker = ClipboardTempFileTracker::new();

    tracker.track(missing.clone());
    fs::remove_file(&missing).ok();
    tracker.cleanup_referenced_in(&missing.display().to_string());
    tracker.cleanup_all();
}
