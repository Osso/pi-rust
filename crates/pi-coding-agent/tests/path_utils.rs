use pi_js_compat::js_json::JsString;
use std::fs;
use std::path::Path;

use pi_coding_agent::core::tools::path_utils::{
    expand_path, path_exists, resolve_read_path, resolve_read_path_async, resolve_to_cwd,
};

fn join(dir: &Path, name: &str) -> JsString {
    dir.join(name).to_str().expect("utf-8 test path").into()
}

#[test]
fn expand_path_expands_home_and_keeps_tilde_prefixed_filenames_literal() {
    assert!(!expand_path("~").unwrap().contains('~'));
    assert!(!expand_path("~/Documents/file.txt").unwrap().contains("~/"));
    assert_eq!(expand_path("~draft.md").unwrap(), "~draft.md");
    assert_eq!(expand_path("@~draft.md").unwrap(), "~draft.md");
}

#[test]
fn expand_path_normalizes_unicode_spaces() {
    assert_eq!(expand_path("file\u{00A0}name.txt").unwrap(), "file name.txt");
}

#[test]
fn resolve_to_cwd_handles_absolute_relative_and_tilde_prefixed_paths() {
    let temp = std::env::temp_dir();
    let absolute = join(&temp, "absolute/path/file.txt");
    assert_eq!(resolve_to_cwd(&absolute, join(&temp, "some/cwd")).unwrap(), absolute);
    assert_eq!(
        resolve_to_cwd("relative/file.txt", "/some/cwd").unwrap(),
        "/some/cwd/relative/file.txt"
    );

    let cwd = join(&temp, "pi-path-utils-cwd");
    let draft = join(&temp, "pi-path-utils-cwd/~draft.md");
    assert_eq!(resolve_to_cwd("~draft.md", &cwd).unwrap(), draft);
    assert_eq!(resolve_to_cwd("@~draft.md", &cwd).unwrap(), draft);
}

#[test]
fn resolve_read_path_returns_existing_files() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("test-file.txt"), "content").unwrap();

    assert_eq!(
        resolve_read_path("test-file.txt", dir.path().to_str().expect("utf-8 test path")).unwrap(),
        join(dir.path(), "test-file.txt")
    );
}

#[test]
fn resolve_read_path_finds_nfd_filenames_from_nfc_input() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("file\u{0065}\u{0301}.txt"), "content").unwrap();

    let result = resolve_read_path("file\u{00e9}.txt", dir.path().to_str().expect("utf-8 test path")).unwrap();

    assert_eq!(result, join(dir.path(), "file\u{0065}\u{0301}.txt"));
}

#[test]
fn resolve_read_path_finds_curly_quote_filenames() {
    let dir = tempfile::tempdir().unwrap();
    let cwd = dir.path().to_str().expect("utf-8 test path").to_owned();
    fs::write(dir.path().join("Capture d\u{2019}cran.txt"), "content").unwrap();
    fs::write(dir.path().join("Capture d\u{2019}\u{00e9}cran.txt"), "content").unwrap();

    assert_eq!(
        resolve_read_path("Capture d'cran.txt", &cwd).unwrap(),
        join(dir.path(), "Capture d\u{2019}cran.txt")
    );
    assert_eq!(
        resolve_read_path("Capture d'\u{00e9}cran.txt", &cwd).unwrap(),
        join(dir.path(), "Capture d\u{2019}\u{00e9}cran.txt")
    );
}

#[test]
fn resolve_read_path_finds_nfd_plus_curly_quote_filenames() {
    let dir = tempfile::tempdir().unwrap();
    let name = "Capture d\u{2019}e\u{0301}cran.txt";
    fs::write(dir.path().join(name), "content").unwrap();

    let result = resolve_read_path(
        "Capture d'\u{00e9}cran.txt",
        dir.path().to_str().expect("utf-8 test path"),
    )
    .unwrap();

    assert_eq!(result, join(dir.path(), name));
}

#[test]
fn resolve_read_path_finds_macos_screenshot_am_pm_variants() {
    let dir = tempfile::tempdir().unwrap();
    let cwd = dir.path().to_str().expect("utf-8 test path").to_owned();
    fs::write(
        dir.path().join("Screenshot 2024-01-01 at 10.00.00\u{202F}AM.png"),
        "content",
    )
    .unwrap();
    fs::write(
        dir.path().join("Screenshot 2024-01-01 at 10.00.00\u{202F}am.png"),
        "content",
    )
    .unwrap();

    assert_eq!(
        resolve_read_path("Screenshot 2024-01-01 at 10.00.00 AM.png", &cwd).unwrap(),
        join(dir.path(), "Screenshot 2024-01-01 at 10.00.00\u{202F}AM.png")
    );
    assert_eq!(
        resolve_read_path("Screenshot 2024-01-01 at 10.00.00 am.png", &cwd).unwrap(),
        join(dir.path(), "Screenshot 2024-01-01 at 10.00.00\u{202F}am.png")
    );
}

#[test]
fn resolve_read_path_returns_the_resolved_path_when_no_variant_exists() {
    let dir = tempfile::tempdir().unwrap();

    let result = resolve_read_path("missing 1 PM.txt", dir.path().to_str().expect("utf-8 test path")).unwrap();

    assert_eq!(result, join(dir.path(), "missing 1 PM.txt"));
}

#[tokio::test]
async fn async_variants_match_the_sync_resolution() {
    let dir = tempfile::tempdir().unwrap();
    let cwd = dir.path().to_str().expect("utf-8 test path").to_owned();
    fs::write(dir.path().join("Capture d\u{2019}cran.txt"), "content").unwrap();

    assert_eq!(
        resolve_read_path_async("Capture d'cran.txt", &cwd).await.unwrap(),
        join(dir.path(), "Capture d\u{2019}cran.txt")
    );
    assert!(path_exists(&join(dir.path(), "Capture d\u{2019}cran.txt")).await);
    assert!(!path_exists(&join(dir.path(), "missing")).await);
}
