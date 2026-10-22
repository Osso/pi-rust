use pi_js_compat::js_json::{JsString, encode_node_utf8};
use std::collections::VecDeque;
use std::path::Path;
use std::sync::Mutex;

use pi_coding_agent::utils::git_worktree::{
    GitExec, GitExecError, GitOutput, ProcessGitExec, WorktreeStartupError, resolve_worktree,
    resolve_worktree_with_exec,
};
use tempfile::TempDir;

#[derive(Default)]
struct MockGitExec {
    results: Mutex<VecDeque<Result<GitOutput, GitExecError>>>,
    calls: Mutex<Vec<(Vec<JsString>, JsString)>>,
}

impl MockGitExec {
    fn with_results(results: Vec<Result<GitOutput, GitExecError>>) -> Self {
        Self {
            results: Mutex::new(results.into()),
            calls: Mutex::new(Vec::new()),
        }
    }

    fn calls(&self) -> Vec<Vec<JsString>> {
        self.calls
            .lock()
            .expect("calls lock")
            .iter()
            .map(|(args, _)| args.clone())
            .collect()
    }
}

impl GitExec for MockGitExec {
    fn exec(&self, args: &[JsString], cwd: &JsString) -> Result<GitOutput, GitExecError> {
        self.calls
            .lock()
            .expect("calls lock")
            .push((args.to_vec(), cwd.clone()));
        self.results
            .lock()
            .expect("results lock")
            .pop_front()
            .unwrap_or_else(|| panic!("unexpected git call: {args:?}"))
    }
}

fn success(stdout: &str) -> Result<GitOutput, GitExecError> {
    Ok(GitOutput {
        stdout: stdout.as_bytes().to_vec(),
        stderr: Vec::new(),
    })
}

fn failure(message: &str, stderr: &str) -> Result<GitOutput, GitExecError> {
    Err(GitExecError {
        message: JsString::from(message),
        stderr: Some(JsString::from(stderr)),
        stdout: None,
    })
}

#[test]
fn reuses_an_existing_sibling_worktree() {
    let git = MockGitExec::with_results(vec![
        success("/repo/project\n"),
        success("worktree /repo/project\nHEAD abc\n\nworktree /repo/project-feature\nHEAD def\n"),
    ]);

    let worktree = resolve_worktree_with_exec(&JsString::from("feature"), &JsString::from("/repo/project"), &git)
        .expect("existing worktree");
    assert_eq!(worktree, JsString::from("/repo/project-feature"));
    assert_eq!(
        git.calls(),
        vec![
            vec!["rev-parse".to_owned(), "--show-toplevel".to_owned()],
            vec!["worktree".to_owned(), "list".to_owned(), "--porcelain".to_owned()],
        ]
    );
}

#[test]
fn creates_a_new_sibling_worktree_from_origin_main() {
    let git = MockGitExec::with_results(vec![
        success("/repo/project\n"),
        success("worktree /repo/project\nHEAD abc\n"),
        success("origin/main\n"),
        success(""),
    ]);

    let worktree = resolve_worktree_with_exec(
        &JsString::from("feature"),
        &JsString::from("/repo/project/subdir"),
        &git,
    )
    .expect("created worktree");
    assert_eq!(worktree, JsString::from("/repo/project-feature"));
    assert_eq!(
        git.calls(),
        vec![
            vec!["rev-parse".to_owned(), "--show-toplevel".to_owned()],
            vec!["worktree".to_owned(), "list".to_owned(), "--porcelain".to_owned()],
            vec!["rev-parse".to_owned(), "--verify".to_owned(), "origin/main".to_owned()],
            vec![
                "worktree".to_owned(),
                "add".to_owned(),
                "/repo/project-feature".to_owned(),
                "origin/main".to_owned()
            ],
        ]
    );
}

#[test]
fn falls_back_to_origin_master() {
    let git = MockGitExec::with_results(vec![
        success("/repo/project\n"),
        success("worktree /repo/project\nHEAD abc\n"),
        Err(GitExecError {
            message: JsString::from("missing main"),
            stderr: None,
            stdout: None,
        }),
        success("origin/master\n"),
        success(""),
    ]);

    let worktree = resolve_worktree_with_exec(&JsString::from("feature"), &JsString::from("/repo/project"), &git)
        .expect("worktree");
    assert_eq!(worktree, JsString::from("/repo/project-feature"));
    assert!(git.calls().contains(&vec![
        JsString::from("worktree"),
        JsString::from("add"),
        JsString::from("/repo/project-feature"),
        JsString::from("origin/master")
    ]));
}

#[test]
fn reports_missing_supported_remote_base() {
    let git = MockGitExec::with_results(vec![
        success("/repo/project\n"),
        success("worktree /repo/project\nHEAD abc\n"),
        failure("missing main", ""),
        failure("missing master", ""),
    ]);

    let error = resolve_worktree_with_exec(&JsString::from("feature"), &JsString::from("/repo/project"), &git)
        .expect_err("missing base");
    assert_eq!(
        error,
        WorktreeStartupError::new("No origin/main or origin/master ref found for worktree creation")
    );
}

#[test]
fn surfaces_git_stderr_when_the_startup_directory_is_not_a_repository() {
    let git = MockGitExec::with_results(vec![failure(
        "Command failed: git rev-parse --show-toplevel",
        "fatal: not a git repository",
    )]);

    let error = resolve_worktree_with_exec(&JsString::from("feature"), &JsString::from("/repo/project"), &git)
        .expect_err("not a repository");
    assert!(error.to_string().contains("fatal: not a git repository"), "{error}");
}

#[test]
fn surfaces_git_stderr_when_worktree_creation_fails() {
    let git = MockGitExec::with_results(vec![
        success("/repo/project\n"),
        success("worktree /repo/project\nHEAD abc\n"),
        success("origin/main\n"),
        failure(
            "Command failed: git worktree add /repo/project-feature origin/main",
            "fatal: 'feature' is already checked out",
        ),
    ]);

    let error = resolve_worktree_with_exec(&JsString::from("feature"), &JsString::from("/repo/project"), &git)
        .expect_err("creation failure");
    assert!(
        error.to_string().contains("fatal: 'feature' is already checked out"),
        "{error}"
    );
}

#[test]
fn creates_and_reuses_a_real_worktree() {
    let dir = TempDir::new().expect("temp dir");
    let repo = dir.path().join("project");
    std::fs::create_dir(&repo).expect("create repo");

    let git = |arguments: &[&str]| {
        let output = std::process::Command::new("git")
            .args(arguments)
            .current_dir(&repo)
            .output()
            .expect("git must run");
        assert!(
            output.status.success(),
            "git {arguments:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        output
    };

    git(&["init", "--quiet"]);
    git(&[
        "-c",
        "user.email=test@example.com",
        "-c",
        "user.name=Test",
        "commit",
        "--allow-empty",
        "--quiet",
        "-m",
        "init",
    ]);
    git(&["update-ref", "refs/remotes/origin/main", "HEAD"]);

    let worktree = resolve_worktree(
        &JsString::from("feature"),
        &JsString::from(repo.to_str().expect("UTF-8 temp path")),
    )
    .expect("create worktree");
    assert_eq!(
        worktree,
        JsString::from(dir.path().join("project-feature").to_str().expect("UTF-8 temp path"))
    );
    assert!(Path::new(&encode_node_utf8(&worktree)).is_dir());

    let reused = resolve_worktree(
        &JsString::from("feature"),
        &JsString::from(repo.to_str().expect("UTF-8 temp path")),
    )
    .expect("reuse worktree");
    assert_eq!(reused, worktree);
}

#[test]
fn real_git_reports_missing_origin_refs() {
    let dir = TempDir::new().expect("temp dir");
    let repo = dir.path().join("project");
    std::fs::create_dir(&repo).expect("create repo");
    let status = std::process::Command::new("git")
        .args(["init", "--quiet"])
        .current_dir(&repo)
        .status()
        .expect("git init");
    assert!(status.success());

    let error = resolve_worktree(
        &JsString::from("feature"),
        &JsString::from(repo.to_str().expect("UTF-8 temp path")),
    )
    .expect_err("missing origin refs");
    assert!(error.to_string().contains("No origin/main or origin/master"), "{error}");
}

#[test]
fn process_exec_reports_non_zero_exit_status() {
    let error = ProcessGitExec
        .exec(
            &[
                JsString::from("rev-parse"),
                JsString::from("--verify"),
                JsString::from("definitely-missing-ref"),
            ],
            &JsString::from("/tmp"),
        )
        .expect_err("git failure");
    assert!(error.message.contains("git rev-parse"), "{:?}", error.message);
}
