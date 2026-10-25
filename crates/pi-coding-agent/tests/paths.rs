#![cfg(unix)]

use std::fs;
use std::os::unix::fs::symlink;
use std::path::PathBuf;

use pi_coding_agent::utils::paths::{
    PathInputOptions, canonicalize_path, format_path_relative_to_cwd_or_absolute, get_cwd_relative_path, is_local_path,
    normalize_path, resolve_path,
};
use pi_js_compat::js_json::JsString;
use tempfile::TempDir;

fn options() -> PathInputOptions {
    PathInputOptions::default()
}

fn text(path: impl Into<PathBuf>) -> String {
    path.into().to_string_lossy().into_owned()
}

#[test]
fn canonicalizes_regular_files_and_symlinks() {
    let dir = TempDir::new().expect("temp dir");
    let file = dir.path().join("file.txt");
    fs::write(&file, "hello").expect("write file");

    assert_eq!(canonicalize_path(text(&file)), text(fs::canonicalize(&file).unwrap()));

    let link = dir.path().join("link.txt");
    symlink(&file, &link).expect("symlink");
    assert_eq!(canonicalize_path(text(&link)), text(fs::canonicalize(&file).unwrap()));

    let target_dir = dir.path().join("target-dir");
    fs::create_dir(&target_dir).expect("create dir");
    let link_dir = dir.path().join("link-dir");
    symlink(&target_dir, &link_dir).expect("symlink dir");
    assert_eq!(
        canonicalize_path(text(&link_dir)),
        text(fs::canonicalize(&target_dir).unwrap())
    );
}

#[test]
fn falls_back_to_the_raw_path_for_missing_targets_and_dangling_links() {
    let dir = TempDir::new().expect("temp dir");
    let nonexistent = text(dir.path().join("no-such-file"));
    assert_eq!(canonicalize_path(&nonexistent), nonexistent);

    let link = dir.path().join("link.txt");
    symlink(dir.path().join("missing-target"), &link).expect("dangling symlink");
    assert_eq!(canonicalize_path(text(&link)), text(&link));
}

#[test]
fn identifies_local_paths() {
    for value in ["my-package", "./foo", "file:///tmp/foo", "/tmp/foo", "  spaced  "] {
        assert!(is_local_path(value), "{value}");
    }
    for value in [
        "npm:package",
        "git://repo",
        "github:user/repo",
        "http://example.com",
        "https://example.com",
        "ssh://host/repo",
    ] {
        assert!(!is_local_path(value), "{value}");
    }
}

#[test]
fn expands_only_home_tilde_shortcuts() {
    let home = "/home/example";
    let with_home = PathInputOptions {
        home_dir: Some(home.into()),
        ..PathInputOptions::default()
    };

    assert_eq!(normalize_path("~", &with_home).unwrap(), home);
    assert_eq!(
        normalize_path("~/file.txt", &with_home).unwrap(),
        "/home/example/file.txt"
    );
    assert_eq!(normalize_path("~draft.md", &with_home).unwrap(), "~draft.md");
    assert_eq!(
        normalize_path(
            "~",
            &PathInputOptions {
                expand_tilde: Some(false),
                home_dir: Some(home.into()),
                ..PathInputOptions::default()
            }
        )
        .unwrap(),
        "~"
    );
}

#[test]
fn applies_trim_strip_at_and_unicode_space_options() {
    let trimmed = PathInputOptions {
        trim: true,
        ..PathInputOptions::default()
    };
    assert_eq!(normalize_path("  /tmp/file  ", &trimmed).unwrap(), "/tmp/file");

    let stripped = PathInputOptions {
        strip_at_prefix: true,
        ..PathInputOptions::default()
    };
    assert_eq!(normalize_path("@/tmp/file", &stripped).unwrap(), "/tmp/file");
    assert_eq!(normalize_path("@file", &PathInputOptions::default()).unwrap(), "@file");

    let unicode = PathInputOptions {
        normalize_unicode_spaces: true,
        ..PathInputOptions::default()
    };
    assert_eq!(
        normalize_path("a\u{a0}b\u{2000}c\u{3000}d", &unicode).unwrap(),
        "a b c d"
    );
}

#[test]
fn accepts_file_urls_and_rejects_malformed_ones() {
    assert_eq!(
        normalize_path("file:///tmp/file%20with%20spaces.txt", &options()).unwrap(),
        "/tmp/file with spaces.txt"
    );
    assert_eq!(
        normalize_path("file:///tmp/report%252026.md", &options()).unwrap(),
        "/tmp/report%2026.md"
    );
    assert!(normalize_path("file:///%E0%A4%A", &options()).is_err());
}

#[test]
fn resolves_relative_paths_against_the_base_directory() {
    let base = "/tmp/pi-paths-cwd";
    assert_eq!(
        resolve_path("subdir/file.txt", base, &options()).unwrap(),
        "/tmp/pi-paths-cwd/subdir/file.txt"
    );
    assert_eq!(
        resolve_path("file:///tmp/pi-paths-cwd", base, &options()).unwrap(),
        "/tmp/pi-paths-cwd"
    );
    assert_eq!(resolve_path("/tmp/a/../b/./c", base, &options()).unwrap(), "/tmp/b/c");
}

#[test]
fn preserves_literal_percent_sequences_in_posix_paths() {
    let base = "/tmp/base";
    for path in ["/tmp/report%2026.md", "/tmp/foo%2Fbar", "/tmp/malformed%A.md"] {
        assert_eq!(resolve_path(path, base, &options()).unwrap(), path);
    }
}

#[test]
fn computes_cwd_relative_paths_and_rejects_traversal() {
    let cwd = "/tmp/pi-paths-cwd";
    assert_eq!(
        get_cwd_relative_path("/tmp/pi-paths-cwd/..config/AGENTS.md", cwd).unwrap(),
        Some(JsString::from("..config/AGENTS.md"))
    );
    assert_eq!(
        get_cwd_relative_path("/tmp/pi-paths-cwd", cwd).unwrap(),
        Some(JsString::from("."))
    );
    assert_eq!(
        get_cwd_relative_path("/tmp/pi-paths-cwd/sub/file", cwd).unwrap(),
        Some(JsString::from("sub/file"))
    );
    assert_eq!(get_cwd_relative_path("/tmp/AGENTS.md", cwd).unwrap(), None);
    assert_eq!(
        get_cwd_relative_path("/tmp/pi-paths-cwd/../AGENTS.md", cwd).unwrap(),
        None
    );
}

#[test]
fn formats_paths_relative_to_cwd_or_absolute() {
    let cwd = "/tmp/pi-paths-cwd";
    assert_eq!(
        format_path_relative_to_cwd_or_absolute("sub/file", cwd).unwrap(),
        "sub/file"
    );
    assert_eq!(
        format_path_relative_to_cwd_or_absolute("/tmp/other/file", cwd).unwrap(),
        "/tmp/other/file"
    );
}

#[test]
fn cloud_sync_marking_is_best_effort() {
    let dir = TempDir::new().expect("temp dir");
    pi_coding_agent::utils::paths::mark_path_ignored_by_cloud_sync(text(dir.path())).unwrap();
}
