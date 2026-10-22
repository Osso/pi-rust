use std::fs;
use std::path::PathBuf;

use pi_coding_agent::utils::clipboard_temp_files::ClipboardTempFileTracker;
use pi_js_compat::js_json::{JsString, encode_node_utf8};
use tempfile::TempDir;

fn create_file(dir: &TempDir, name: &str) -> PathBuf {
    let path = dir.path().join(name);
    fs::write(&path, "data").expect("write file");
    path
}

fn js_path(path: &std::path::Path) -> JsString {
    path.to_str().expect("UTF-8 fixture path").into()
}

#[test]
fn deletes_tracked_files_referenced_by_submitted_text() {
    let dir = TempDir::new().expect("temp dir");
    let pasted = create_file(&dir, "pi-clipboard-image.png");
    let unrelated = create_file(&dir, "unrelated.png");
    let mut tracker = ClipboardTempFileTracker::new();

    tracker.track(js_path(&pasted));
    tracker.cleanup_referenced_in(&(JsString::from("describe ") + &js_path(&pasted)));

    assert!(!pasted.exists());
    assert!(unrelated.exists());
}

#[test]
fn keeps_tracked_files_until_text_references_them() {
    let dir = TempDir::new().expect("temp dir");
    let pasted = create_file(&dir, "pi-clipboard-image.png");
    let mut tracker = ClipboardTempFileTracker::new();

    tracker.track(js_path(&pasted));
    tracker.cleanup_referenced_in(&"different prompt".into());
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

    tracker.track(js_path(&first));
    tracker.track(js_path(&second));
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

    tracker.track(js_path(&missing));
    fs::remove_file(&missing).ok();
    tracker.cleanup_referenced_in(&js_path(&missing));
    tracker.cleanup_all();
}

#[test]
fn matches_submitted_text_by_raw_utf16_units_and_unlinks_the_node_encoded_path() {
    let dir = TempDir::new().expect("temp dir");
    let tracked = js_path(dir.path()) + "/clip-" + JsString::from_utf16(vec![0xd800]) + ".png";
    // Node writes and unlinks the lone surrogate as U+FFFD bytes.
    let on_disk = PathBuf::from(encode_node_utf8(&tracked));
    fs::write(&on_disk, "data").expect("write file");
    let mut tracker = ClipboardTempFileTracker::new();
    tracker.track(tracked.clone());

    let repaired_text = JsString::from(encode_node_utf8(&tracked));
    tracker.cleanup_referenced_in(&repaired_text);
    assert!(on_disk.exists(), "U+FFFD text must not match the raw tracked path");

    tracker.cleanup_referenced_in(&(JsString::from("see ") + &tracked));
    assert!(!on_disk.exists());
}
