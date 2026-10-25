use std::process::{Command, Stdio};

use pi_js_compat::js_format;
use pi_js_compat::js_json::{JsString, encode_node_utf8};

use crate::utils::paths::{node_basename, node_dirname};

/// Worktree startup failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorktreeStartupError {
    message: JsString,
}

impl WorktreeStartupError {
    pub fn new(message: impl Into<JsString>) -> Self {
        Self {
            message: message.into(),
        }
    }

    pub fn message(&self) -> &JsString {
        &self.message
    }
}

impl std::fmt::Display for WorktreeStartupError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&encode_node_utf8(&self.message))
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
    pub message: JsString,
    pub stderr: Option<JsString>,
    pub stdout: Option<JsString>,
}

/// Runs git commands for worktree resolution.
pub trait GitExec {
    fn exec(&self, args: &[JsString], cwd: &JsString) -> Result<GitOutput, GitExecError>;
}

/// Runs git through the local `git` executable.
pub struct ProcessGitExec;

impl GitExec for ProcessGitExec {
    fn exec(&self, args: &[JsString], cwd: &JsString) -> Result<GitOutput, GitExecError> {
        let output = Command::new("git")
            .args(args.iter().map(encode_node_utf8))
            .current_dir(encode_node_utf8(cwd))
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .map_err(|error| GitExecError {
                message: error.to_string().into(),
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
            message: js_format!("Command failed: git {}", JsString::join(args, " ")),
            stderr: Some(output_text(&output.stderr)),
            stdout: Some(output_text(&output.stdout)),
        })
    }
}

/// `output.toString("utf8")`.
fn output_text(output: &[u8]) -> JsString {
    String::from_utf8_lossy(output).as_ref().into()
}

/// Resolve a sibling worktree, creating it from `origin/main` or
/// `origin/master` when it does not exist.
pub fn resolve_worktree(name: &JsString, cwd: &JsString) -> Result<JsString, WorktreeStartupError> {
    resolve_worktree_with_exec(name, cwd, &ProcessGitExec)
}

/// Resolve a sibling worktree using a supplied git executor.
pub fn resolve_worktree_with_exec(
    name: &JsString,
    cwd: &JsString,
    exec: &dyn GitExec,
) -> Result<JsString, WorktreeStartupError> {
    let repo_root = git_output(exec, &["rev-parse", "--show-toplevel"], cwd)
        .map_err(|error| WorktreeStartupError::new(js_format!("Unable to find git repository: {}", error.message())))?;

    let target_path = js_format!("{}/{}-{}", node_dirname(&repo_root), node_basename(&repo_root), name);
    let worktree_list = git_output(exec, &["worktree", "list", "--porcelain"], &repo_root)?;
    if parse_worktree_paths(&worktree_list).contains(&target_path) {
        return Ok(target_path);
    }

    let base_ref = resolve_base_ref(exec, &repo_root)?;
    git_output_args(
        exec,
        &["worktree".into(), "add".into(), target_path.clone(), base_ref.into()],
        &repo_root,
    )?;
    Ok(target_path)
}

fn resolve_base_ref(exec: &dyn GitExec, repo_root: &JsString) -> Result<&'static str, WorktreeStartupError> {
    if git_output(exec, &["rev-parse", "--verify", "origin/main"], repo_root).is_ok() {
        return Ok("origin/main");
    }
    if git_output(exec, &["rev-parse", "--verify", "origin/master"], repo_root).is_ok() {
        return Ok("origin/master");
    }
    Err(WorktreeStartupError::new(
        "No origin/main or origin/master ref found for worktree creation",
    ))
}

fn git_output(exec: &dyn GitExec, args: &[&str], cwd: &JsString) -> Result<JsString, WorktreeStartupError> {
    let args: Vec<JsString> = args.iter().map(|arg| JsString::from(*arg)).collect();
    git_output_args(exec, &args, cwd)
}

fn git_output_args(exec: &dyn GitExec, args: &[JsString], cwd: &JsString) -> Result<JsString, WorktreeStartupError> {
    exec.exec(args, cwd)
        .map(|output| output_text(&output.stdout).trim())
        .map_err(|error| WorktreeStartupError::new(format_git_error(&error)))
}

fn format_git_error(error: &GitExecError) -> JsString {
    for output in [&error.stderr, &error.stdout].into_iter().flatten() {
        let output = output.trim();
        if !output.is_empty() {
            return output;
        }
    }
    error.message.clone()
}

/// `output.split(/\r?\n/)` worktree entries.
fn parse_worktree_paths(output: &JsString) -> Vec<JsString> {
    output
        .split('\n')
        .map(|line| line.strip_suffix("\r").unwrap_or(line))
        .filter_map(|line| line.strip_prefix("worktree "))
        .map(|path| path.trim())
        .filter(|path| !path.is_empty())
        .collect()
}
