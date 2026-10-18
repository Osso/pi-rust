use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Worktree startup failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorktreeStartupError {
    message: String,
}

impl WorktreeStartupError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl std::fmt::Display for WorktreeStartupError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for WorktreeStartupError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitOutput {
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitExecError {
    pub message: String,
    pub stderr: Option<String>,
    pub stdout: Option<String>,
}

/// Runs git commands for worktree resolution.
pub trait GitExec {
    fn exec(&self, args: &[&str], cwd: &Path) -> Result<GitOutput, GitExecError>;
}

/// Runs git through the local `git` executable.
pub struct ProcessGitExec;

impl GitExec for ProcessGitExec {
    fn exec(&self, args: &[&str], cwd: &Path) -> Result<GitOutput, GitExecError> {
        let output = Command::new("git")
            .args(args)
            .current_dir(cwd)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .map_err(|error| GitExecError {
                message: error.to_string(),
                stderr: None,
                stdout: None,
            })?;

        if output.status.success() {
            return Ok(GitOutput {
                stdout: output.stdout,
                stderr: output.stderr,
            });
        }

        Err(GitExecError {
            message: format!("Command failed: git {}", args.join(" ")),
            stderr: Some(String::from_utf8_lossy(&output.stderr).into_owned()),
            stdout: Some(String::from_utf8_lossy(&output.stdout).into_owned()),
        })
    }
}

/// Resolve a sibling worktree, creating it from `origin/main` or
/// `origin/master` when it does not exist.
pub fn resolve_worktree(name: &str, cwd: &Path) -> Result<PathBuf, WorktreeStartupError> {
    resolve_worktree_with_exec(name, cwd, &ProcessGitExec)
}

/// Resolve a sibling worktree using a supplied git executor.
pub fn resolve_worktree_with_exec(name: &str, cwd: &Path, exec: &dyn GitExec) -> Result<PathBuf, WorktreeStartupError> {
    let repo_root = git_output(exec, &["rev-parse", "--show-toplevel"], cwd)
        .map_err(|error| WorktreeStartupError::new(format!("Unable to find git repository: {error}")))?;

    let target_path = sibling_worktree_path(&repo_root, name);
    let worktree_list = git_output(exec, &["worktree", "list", "--porcelain"], Path::new(&repo_root))?;
    if parse_worktree_paths(&worktree_list).contains(&target_path.to_string_lossy().into_owned()) {
        return Ok(target_path);
    }

    let base_ref = resolve_base_ref(exec, Path::new(&repo_root))?;
    git_output(
        exec,
        &["worktree", "add", &target_path.to_string_lossy(), &base_ref],
        Path::new(&repo_root),
    )?;
    Ok(target_path)
}

fn sibling_worktree_path(repo_root: &str, name: &str) -> PathBuf {
    let repo_root = Path::new(repo_root);
    let parent = repo_root.parent().unwrap_or_else(|| Path::new("/"));
    let base_name = repo_root
        .file_name()
        .map(|value| value.to_string_lossy().into_owned())
        .unwrap_or_default();
    parent.join(format!("{base_name}-{name}"))
}

fn resolve_base_ref(exec: &dyn GitExec, repo_root: &Path) -> Result<String, WorktreeStartupError> {
    if git_output(exec, &["rev-parse", "--verify", "origin/main"], repo_root).is_ok() {
        return Ok("origin/main".to_owned());
    }
    if git_output(exec, &["rev-parse", "--verify", "origin/master"], repo_root).is_ok() {
        return Ok("origin/master".to_owned());
    }
    Err(WorktreeStartupError::new(
        "No origin/main or origin/master ref found for worktree creation",
    ))
}

fn git_output(exec: &dyn GitExec, args: &[&str], cwd: &Path) -> Result<String, WorktreeStartupError> {
    exec.exec(args, cwd)
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned())
        .map_err(|error| WorktreeStartupError::new(format_git_error(&error)))
}

fn format_git_error(error: &GitExecError) -> String {
    if let Some(stderr) = &error.stderr {
        let stderr = stderr.trim();
        if !stderr.is_empty() {
            return stderr.to_owned();
        }
    }
    if let Some(stdout) = &error.stdout {
        let stdout = stdout.trim();
        if !stdout.is_empty() {
            return stdout.to_owned();
        }
    }
    error.message.clone()
}

fn parse_worktree_paths(output: &str) -> Vec<String> {
    output
        .split('\n')
        .map(|line| line.strip_suffix('\r').unwrap_or(line))
        .filter(|line| line.starts_with("worktree "))
        .map(|line| line["worktree ".len()..].trim().to_owned())
        .filter(|line| !line.is_empty())
        .collect()
}
