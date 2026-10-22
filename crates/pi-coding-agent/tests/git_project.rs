use std::process::Command;

use pi_coding_agent::utils::git_project::read_git_common_directory;
use pi_js_compat::js_json::JsString;
use tempfile::TempDir;

fn raw_path(path: &std::path::Path) -> JsString {
    JsString::from(path.to_str().expect("UTF-8 temp path"))
}

fn run_git(cwd: &std::path::Path, arguments: &[&str]) -> std::process::Output {
    let output = Command::new("git")
        .args(arguments)
        .current_dir(cwd)
        .output()
        .expect("git must run");
    assert!(
        output.status.success(),
        "git {arguments:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

#[test]
fn reads_the_common_git_directory() {
    let dir = TempDir::new().expect("temp dir");
    run_git(dir.path(), &["init", "--quiet"]);

    let common_directory = read_git_common_directory(&raw_path(dir.path())).expect("git common directory");
    assert_eq!(common_directory, raw_path(&dir.path().join(".git")));
}

#[test]
fn resolves_the_common_directory_from_a_subdirectory() {
    let dir = TempDir::new().expect("temp dir");
    run_git(dir.path(), &["init", "--quiet"]);
    let subdirectory = dir.path().join("nested");
    std::fs::create_dir(&subdirectory).expect("create subdirectory");

    let common_directory = read_git_common_directory(&raw_path(&subdirectory)).expect("git common directory");
    assert_eq!(common_directory, raw_path(&dir.path().join(".git")));
}

#[test]
fn returns_none_outside_a_git_repository() {
    let dir = TempDir::new().expect("temp dir");
    assert_eq!(read_git_common_directory(&raw_path(dir.path())), None);
}
