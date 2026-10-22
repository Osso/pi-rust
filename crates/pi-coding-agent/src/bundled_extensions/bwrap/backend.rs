//! Port of `packages/coding-agent/extensions/bwrap/src/backend.ts`.

use indexmap::IndexMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use pi_agent_core::agent::ToolError;
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tokio_util::sync::CancellationToken;

use crate::core::permissions::presets::SandboxProfileName;
use crate::core::tools::bash::{
    BashDataCallback, BashExecOptions, BashExecResult, BashOperations, LocalBashOperations,
};
use crate::utils::child_process::node_spawn_error_message;

pub const SANDBOX_HOME: &str = "/tmp/pi-home";
const DEFAULT_LANG: &str = "C.UTF-8";
const DEFAULT_PATH: &str = "/usr/local/sbin:/usr/local/bin:/usr/bin:/bin";
const SHEBANG_PREFIX_BYTES: usize = 4096;
const PYTHON_LIBRARY_DIRECTORIES: [&str; 2] = ["lib", "lib64"];
const READ_ONLY_SYSTEM_PATHS: [&str; 7] = ["/usr", "/bin", "/sbin", "/lib", "/lib64", "/etc", "/nix"];
const REQUIRED_SYSTEM_SYMLINKS: [&str; 1] = ["/etc/resolv.conf"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BwrapSandboxProfile(pub SandboxProfileName);

impl BwrapSandboxProfile {
    pub fn as_str(self) -> &'static str {
        self.0.as_str()
    }
}

#[derive(Debug, Clone)]
pub struct BwrapInvocationOptions {
    pub bwrap_command: String,
    pub command: Vec<pi_js_compat::js_json::JsString>,
    pub cwd: String,
    pub env: Option<IndexMap<String, String>>,
    pub extra_read_only_paths: Option<Vec<String>>,
    pub home_dir: Option<String>,
    pub profile: BwrapSandboxProfile,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BwrapInvocation {
    pub argv: Vec<pi_js_compat::js_json::JsString>,
    pub command: String,
    pub env: IndexMap<String, String>,
}

/// `resolveBwrapSandboxProfile`.
pub fn resolve_bwrap_sandbox_profile(profile: SandboxProfileName) -> Option<BwrapSandboxProfile> {
    if profile == SandboxProfileName::FullAccess {
        None
    } else {
        Some(BwrapSandboxProfile(profile))
    }
}

/// `runBwrapAvailabilityCheck`.
pub fn run_bwrap_availability_check(bwrap_command: &str) -> Result<(), String> {
    let result = std::process::Command::new(bwrap_command)
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output();
    match result {
        Err(error) => Err(format!(
            "bubblewrap is required for sandbox profile but is unavailable: {}",
            spawn_sync_error(bwrap_command, &error)
        )),
        Ok(output) if !output.status.success() => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stderr = stderr.trim();
            if stderr.is_empty() {
                Err("bubblewrap is required for sandbox profile but failed to run".to_owned())
            } else {
                Err(format!(
                    "bubblewrap is required for sandbox profile but failed to run: {stderr}"
                ))
            }
        }
        Ok(_) => Ok(()),
    }
}

fn spawn_sync_error(command: &str, error: &std::io::Error) -> String {
    node_spawn_error_message("spawnSync", command, error)
}

/// `buildBwrapInvocation`.
pub fn build_bwrap_invocation(options: &BwrapInvocationOptions) -> Result<BwrapInvocation, String> {
    let cwd = absolute_path(&options.cwd);
    let sandbox_env = build_sandbox_env(options.env.as_ref());
    Ok(BwrapInvocation {
        argv: build_bwrap_arguments(options, &cwd, &sandbox_env),
        command: options.bwrap_command.clone(),
        env: sandbox_env,
    })
}

fn build_bwrap_arguments(
    options: &BwrapInvocationOptions,
    cwd: &str,
    sandbox_env: &IndexMap<String, String>,
) -> Vec<String> {
    let mut arguments = build_bwrap_base_arguments();
    for (key, value) in sandbox_env {
        arguments.push("--setenv".to_owned());
        arguments.push(key.clone());
        arguments.push(value.clone());
    }
    for system_path in READ_ONLY_SYSTEM_PATHS {
        if Path::new(system_path).exists() {
            arguments.extend(["--ro-bind".to_owned(), system_path.to_owned(), system_path.to_owned()]);
        }
    }
    for path in REQUIRED_SYSTEM_SYMLINKS {
        arguments.extend(build_resolved_read_only_mount_arguments(path, cwd));
    }
    for path in options.extra_read_only_paths.as_deref().unwrap_or_default() {
        arguments.extend(build_read_only_mount_arguments(path, cwd));
    }
    arguments.extend(build_workspace_mount_arguments(cwd, options.profile));
    arguments.push("--chdir".to_owned());
    arguments.push(cwd.to_owned());
    arguments.push("--".to_owned());
    arguments.extend(options.command.iter().cloned());
    arguments
}

fn build_bwrap_base_arguments() -> Vec<String> {
    [
        "--die-with-parent",
        "--unshare-all",
        "--share-net",
        "--clearenv",
        "--dev",
        "/dev",
        "--proc",
        "/proc",
        "--tmpfs",
        "/tmp",
        "--dir",
        SANDBOX_HOME,
    ]
    .into_iter()
    .map(str::to_owned)
    .collect()
}

pub struct SandboxedBashOperations {
    bwrap_command: String,
    profile: Option<BwrapSandboxProfile>,
    shell_path: Option<String>,
    local: LocalBashOperations,
}

/// `createSandboxedBashOperations`.
pub fn create_sandboxed_bash_operations(
    bwrap_command: String,
    profile: Option<BwrapSandboxProfile>,
    shell_path: Option<String>,
) -> Arc<dyn BashOperations> {
    Arc::new(SandboxedBashOperations {
        bwrap_command,
        profile,
        shell_path,
        local: LocalBashOperations::default(),
    })
}

impl BashOperations for SandboxedBashOperations {
    fn exec<'a>(
        &'a self,
        command: &'a pi_js_compat::js_json::JsString,
        cwd: &'a str,
        options: BashExecOptions,
    ) -> crate::core::extensions::types::BoxFuture<'a, Result<BashExecResult, ToolError>> {
        let Some(profile) = self.profile else {
            return self.local.exec(command, cwd, options);
        };
        let bwrap_command = self.bwrap_command.clone();
        let shell_path = self.shell_path.clone().unwrap_or_else(|| "/bin/sh".to_owned());
        Box::pin(async move {
            if options.signal.as_ref().is_some_and(CancellationToken::is_cancelled) {
                return Err(ToolError::new("aborted"));
            }
            run_bwrap_availability_check(&bwrap_command).map_err(ToolError::new)?;
            let invocation = build_bwrap_invocation(&BwrapInvocationOptions {
                bwrap_command,
                command: vec![shell_path.into(), "-lc".into(), command.clone()],
                cwd: cwd.to_owned(),
                env: Some(
                    options
                        .env
                        .iter()
                        .map(|(key, value)| (key.clone(), value.clone()))
                        .collect(),
                ),
                extra_read_only_paths: None,
                home_dir: None,
                profile,
            })
            .map_err(ToolError::new)?;
            execute_bwrap_command(invocation, options).await
        })
    }
}

async fn execute_bwrap_command(
    invocation: BwrapInvocation,
    options: BashExecOptions,
) -> Result<BashExecResult, ToolError> {
    let mut command = Command::new(&invocation.command);
    command
        .args(invocation.argv.iter().map(pi_js_compat::js_json::encode_node_utf8))
        .env_clear()
        .envs(&invocation.env)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if !cfg!(windows) {
        command.process_group(0);
    }
    let mut child = command.spawn().map_err(|error| ToolError::new(error.to_string()))?;
    let child_pid = child.id();
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| ToolError::new("bwrap stdout unavailable"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| ToolError::new("bwrap stderr unavailable"))?;
    let on_data: BashDataCallback = options.on_data.clone();
    let stdout_task = tokio::spawn(forward_output(stdout, Arc::clone(&on_data)));
    let stderr_task = tokio::spawn(forward_output(stderr, on_data));
    let timeout = options.timeout.filter(|timeout| *timeout > 0.0);
    let mut timed_out = false;
    let status = {
        let wait = child.wait();
        tokio::pin!(wait);
        match (timeout, options.signal.clone()) {
            (Some(timeout), signal) => {
                let deadline = tokio::time::sleep(Duration::from_secs_f64(timeout));
                tokio::pin!(deadline);
                tokio::select! {
                    status = &mut wait => status,
                    _ = &mut deadline => {
                        timed_out = true;
                        terminate_child_process(child_pid);
                        wait.await
                    }
                    _ = async {
                        match signal {
                            Some(signal) => signal.cancelled().await,
                            None => std::future::pending::<()>().await,
                        }
                    } => {
                        terminate_child_process(child_pid);
                        wait.await
                    }
                }
            }
            (None, signal) => tokio::select! {
                status = &mut wait => status,
                _ = async {
                    match signal {
                        Some(signal) => signal.cancelled().await,
                        None => std::future::pending::<()>().await,
                    }
                } => {
                    terminate_child_process(child_pid);
                    wait.await
                }
            },
        }
    };
    let _ = stdout_task.await;
    let _ = stderr_task.await;
    if options.signal.as_ref().is_some_and(CancellationToken::is_cancelled) {
        return Err(ToolError::new("aborted"));
    }
    if timed_out {
        return Err(ToolError::new(format!(
            "timeout:{}",
            options.timeout.unwrap_or_default()
        )));
    }
    Ok(BashExecResult {
        exit_code: status.ok().and_then(|status| status.code()),
        detached: None,
    })
}

async fn forward_output(mut stream: impl tokio::io::AsyncRead + Unpin, on_data: BashDataCallback) {
    let mut buffer = [0u8; 8192];
    loop {
        match stream.read(&mut buffer).await {
            Ok(0) | Err(_) => break,
            Ok(read) => on_data(&buffer[..read]),
        }
    }
}

fn terminate_child_process(pid: Option<u32>) {
    #[cfg(unix)]
    {
        use rustix::process::{Pid, Signal, kill_process, kill_process_group};
        let Some(pid) = pid.and_then(|pid| Pid::from_raw(i32::try_from(pid).ok()?)) else {
            return;
        };
        if kill_process_group(pid, Signal::KILL).is_err() {
            let _ = kill_process(pid, Signal::KILL);
        }
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
    }
}

/// `createBwrapRunnerEnvironment`.
pub fn create_bwrap_runner_environment(
    host_env: &IndexMap<String, String>,
    python_path: Option<&str>,
) -> IndexMap<String, String> {
    let mut environment = IndexMap::new();
    if let Some(lang) = host_env.get("LANG") {
        environment.insert("LANG".to_owned(), lang.clone());
    }
    if let Some(path) = host_env.get("PATH") {
        environment.insert("PATH".to_owned(), path.clone());
    }
    if let Some(python_path) = python_path {
        environment.insert("PYTHONPATH".to_owned(), python_path.to_owned());
    }
    if let Some(term) = host_env.get("TERM") {
        environment.insert("TERM".to_owned(), term.clone());
    }
    if let Some(user) = host_env.get("USER") {
        environment.insert("USER".to_owned(), user.clone());
    }
    environment
}

/// `createBwrapRunnerCommand`.
pub fn create_bwrap_runner_command(input: BwrapRunnerCommandOptions) -> Result<BwrapRunnerCommand, String> {
    run_bwrap_availability_check(&input.bwrap_command)?;
    let runner_command = find_runner_executable(&input.runner_command, input.runner_env.as_ref());
    let mut extra_read_only_paths = input.extra_read_only_paths.unwrap_or_default();
    extra_read_only_paths.extend(runner_read_only_paths(
        &runner_command,
        &input.runner_args,
        input.runner_env.as_ref(),
        &input.cwd,
    ));
    let mut command = vec![runner_command];
    command.extend(input.runner_args.iter().cloned());
    let invocation = build_bwrap_invocation(&BwrapInvocationOptions {
        bwrap_command: input.bwrap_command,
        command,
        cwd: input.cwd,
        env: input.runner_env,
        extra_read_only_paths: Some(extra_read_only_paths),
        home_dir: None,
        profile: input.profile,
    })?;
    Ok(BwrapRunnerCommand {
        args: invocation.argv,
        command: invocation.command,
        env: invocation.env,
    })
}

pub struct BwrapRunnerCommandOptions {
    pub bwrap_command: String,
    pub cwd: String,
    pub extra_read_only_paths: Option<Vec<String>>,
    pub profile: BwrapSandboxProfile,
    pub runner_args: Vec<pi_js_compat::js_json::JsString>,
    pub runner_command: String,
    pub runner_env: Option<IndexMap<String, String>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BwrapRunnerCommand {
    pub args: Vec<pi_js_compat::js_json::JsString>,
    pub command: String,
    pub env: IndexMap<String, String>,
}

fn find_runner_executable(command: &str, env: Option<&IndexMap<String, String>>) -> String {
    if is_absolute(command) {
        return resolve_runner_executable(command);
    }
    let search_path = env
        .and_then(|env| env.get("PATH"))
        .cloned()
        .or_else(|| pi_js_compat::process_env::get("PATH"))
        .unwrap_or_else(|| DEFAULT_PATH.to_owned());
    for directory in search_path.split(':') {
        let candidate = Path::new(directory).join(command);
        if candidate.exists() {
            return resolve_runner_executable(&candidate.display().to_string());
        }
    }
    command.to_owned()
}

fn resolve_runner_executable(path: &str) -> String {
    if Path::new(path).exists() {
        std::fs::canonicalize(path)
            .map(|path| path.display().to_string())
            .unwrap_or_else(|_| path.to_owned())
    } else {
        path.to_owned()
    }
}

fn read_runner_runtime_paths(command: &str) -> Vec<String> {
    let Some(interpreter) = read_absolute_shebang_interpreter(command) else {
        return Vec::new();
    };
    if !Path::new(&interpreter).exists() {
        return Vec::new();
    }
    let interpreter_target = std::fs::canonicalize(&interpreter)
        .map(|path| path.display().to_string())
        .unwrap_or_else(|_| interpreter.clone());
    let interpreter_mount_path = if interpreter_target == interpreter {
        interpreter.clone()
    } else {
        Path::new(&interpreter)
            .parent()
            .map(|parent| parent.display().to_string())
            .unwrap_or_else(|| interpreter.clone())
    };
    let virtual_environment_root = Path::new(&interpreter)
        .parent()
        .and_then(Path::parent)
        .map(|path| path.display().to_string())
        .unwrap_or_default();
    let virtual_environment_config = Path::new(&virtual_environment_root).join("pyvenv.cfg");
    if !virtual_environment_config.exists() {
        return vec![interpreter_mount_path, interpreter_target];
    }
    let runtime_root = Path::new(&interpreter_target)
        .parent()
        .and_then(Path::parent)
        .map(|path| path.display().to_string())
        .unwrap_or_default();
    let mut paths = vec![interpreter_mount_path, virtual_environment_config.display().to_string()];
    for directory in PYTHON_LIBRARY_DIRECTORIES {
        paths.push(
            Path::new(&virtual_environment_root)
                .join(directory)
                .display()
                .to_string(),
        );
    }
    paths.push(interpreter_target);
    for directory in PYTHON_LIBRARY_DIRECTORIES {
        paths.push(Path::new(&runtime_root).join(directory).display().to_string());
    }
    paths
}

fn read_absolute_shebang_interpreter(path: &str) -> Option<String> {
    let mut file = std::fs::File::open(path).ok()?;
    let mut prefix = vec![0u8; SHEBANG_PREFIX_BYTES];
    let bytes_read = file.read(&mut prefix).ok()?;
    let prefix = String::from_utf8_lossy(&prefix[..bytes_read]);
    parse_shebang_interpreter(&prefix)
}

fn parse_shebang_interpreter(prefix: &str) -> Option<String> {
    let rest = prefix.strip_prefix("#!")?;
    let rest = rest.trim_start_matches([' ', '\t']);
    let end = rest.find([' ', '\t', '\r', '\n']).unwrap_or(rest.len());
    let interpreter = &rest[..end];
    if interpreter.starts_with('/') {
        Some(interpreter.to_owned())
    } else {
        None
    }
}

fn runner_read_only_paths(
    command: &str,
    args: &[String],
    env: Option<&IndexMap<String, String>>,
    cwd: &str,
) -> Vec<String> {
    let workspace = absolute_path(cwd);
    let mut candidate_paths = read_runner_runtime_paths(command);
    if is_absolute(command) {
        candidate_paths.push(command.to_owned());
    }
    candidate_paths.extend(args.iter().filter(|arg| is_absolute(arg)).cloned());
    if let Some(python_path) = env.and_then(|env| env.get("PYTHONPATH")) {
        candidate_paths.extend(
            python_path
                .split(':')
                .filter(|path| !path.is_empty())
                .map(str::to_owned),
        );
    }
    let resolved_paths: Vec<String> = candidate_paths
        .into_iter()
        .map(|path| resolve_sandbox_path(&path, &workspace))
        .filter(|path| !is_sandbox_mount_unnecessary(path, &workspace))
        .collect();
    let mut unique_paths = Vec::new();
    for path in resolved_paths {
        if !unique_paths.contains(&path) {
            unique_paths.push(path);
        }
    }
    unique_paths
        .iter()
        .filter(|path| {
            !unique_paths
                .iter()
                .any(|parent| parent != *path && path.starts_with(&format!("{parent}/")))
        })
        .cloned()
        .collect()
}

fn resolve_sandbox_path(path: &str, workspace: &str) -> String {
    if is_absolute(path) {
        absolute_path(path)
    } else {
        absolute_path(&Path::new(workspace).join(path).display().to_string())
    }
}

fn is_sandbox_mount_unnecessary(path: &str, workspace: &str) -> bool {
    if !Path::new(path).exists() {
        return true;
    }
    if path == workspace || path.starts_with(&format!("{workspace}/")) {
        return true;
    }
    READ_ONLY_SYSTEM_PATHS
        .iter()
        .any(|system_path| path == *system_path || path.starts_with(&format!("{system_path}/")))
}

fn build_resolved_read_only_mount_arguments(path: &str, workspace: &str) -> Vec<String> {
    if !Path::new(path).exists() {
        return Vec::new();
    }
    let target = std::fs::canonicalize(path)
        .map(|path| path.display().to_string())
        .unwrap_or_else(|_| path.to_owned());
    if is_sandbox_mount_unnecessary(&target, workspace) {
        return Vec::new();
    }
    build_read_only_mount_arguments(&target, workspace)
}

fn build_read_only_mount_arguments(path: &str, workspace: &str) -> Vec<String> {
    let resolved_path = absolute_path(path);
    if !Path::new(&resolved_path).exists() || resolved_path == workspace {
        return Vec::new();
    }
    let mut arguments = create_parent_directory_arguments(&resolved_path);
    arguments.extend(["--ro-bind".to_owned(), resolved_path.clone(), resolved_path]);
    arguments
}

fn build_workspace_mount_arguments(workspace: &str, profile: BwrapSandboxProfile) -> Vec<String> {
    if !Path::new(workspace).exists() {
        return Vec::new();
    }
    let bind_mode = if profile.0 == SandboxProfileName::WorkspaceWrite {
        "--bind"
    } else {
        "--ro-bind"
    };
    let mut arguments = create_parent_directory_arguments(workspace);
    arguments.extend([bind_mode.to_owned(), workspace.to_owned(), workspace.to_owned()]);
    arguments
}

fn create_parent_directory_arguments(path: &str) -> Vec<String> {
    derive_parent_directories(path)
        .into_iter()
        .flat_map(|parent| ["--dir".to_owned(), parent])
        .collect()
}

fn build_sandbox_env(env: Option<&IndexMap<String, String>>) -> IndexMap<String, String> {
    let mut sandbox_env = IndexMap::new();
    sandbox_env.insert("HOME".to_owned(), SANDBOX_HOME.to_owned());
    sandbox_env.insert(
        "LANG".to_owned(),
        env.and_then(|env| env.get("LANG"))
            .cloned()
            .unwrap_or_else(|| DEFAULT_LANG.to_owned()),
    );
    sandbox_env.insert(
        "PATH".to_owned(),
        env.and_then(|env| env.get("PATH"))
            .cloned()
            .unwrap_or_else(|| DEFAULT_PATH.to_owned()),
    );
    sandbox_env.insert("TMPDIR".to_owned(), "/tmp".to_owned());
    sandbox_env.insert(
        "XDG_CONFIG_HOME".to_owned(),
        Path::new(SANDBOX_HOME).join(".config").display().to_string(),
    );
    if let Some(python_path) = env.and_then(|env| env.get("PYTHONPATH")) {
        sandbox_env.insert("PYTHONPATH".to_owned(), python_path.clone());
    }
    if let Some(term) = env.and_then(|env| env.get("TERM")) {
        sandbox_env.insert("TERM".to_owned(), term.clone());
    }
    if let Some(user) = env
        .and_then(|env| env.get("USER"))
        .cloned()
        .or_else(|| pi_js_compat::process_env::get("USER"))
    {
        sandbox_env.insert("USER".to_owned(), user);
    }
    sandbox_env
}

fn derive_parent_directories(path: &str) -> Vec<String> {
    let parts: Vec<&str> = path.split('/').filter(|part| !part.is_empty()).collect();
    (0..parts.len().saturating_sub(1))
        .map(|index| format!("/{}", parts[..=index].join("/")))
        .collect()
}

fn is_absolute(path: &str) -> bool {
    Path::new(path).is_absolute()
}

fn absolute_path(path: &str) -> String {
    std::path::absolute(path)
        .map(|path| path.display().to_string())
        .unwrap_or_else(|_| PathBuf::from(path).display().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_profiles() {
        assert!(resolve_bwrap_sandbox_profile(SandboxProfileName::FullAccess).is_none());
        assert_eq!(
            resolve_bwrap_sandbox_profile(SandboxProfileName::ReadOnly),
            Some(BwrapSandboxProfile(SandboxProfileName::ReadOnly))
        );
    }

    #[test]
    fn derives_parent_directories() {
        assert_eq!(derive_parent_directories("/a/b/c"), vec!["/a", "/a/b"]);
        assert_eq!(derive_parent_directories("/a"), Vec::<String>::new());
    }

    #[test]
    fn builds_runner_environment_in_source_order() {
        let mut host = IndexMap::new();
        host.insert("LANG".to_owned(), "C.UTF-8".to_owned());
        host.insert("PATH".to_owned(), "/bin".to_owned());
        host.insert("TERM".to_owned(), "xterm".to_owned());
        host.insert("USER".to_owned(), "tester".to_owned());
        let env = create_bwrap_runner_environment(&host, Some("/py"));
        let keys: Vec<&str> = env.keys().map(String::as_str).collect();
        assert_eq!(keys, vec!["LANG", "PATH", "PYTHONPATH", "TERM", "USER"]);
    }

    #[test]
    fn parses_shebang_interpreter() {
        assert_eq!(
            parse_shebang_interpreter("#!/usr/bin/python3 -u\n").as_deref(),
            Some("/usr/bin/python3")
        );
        assert_eq!(
            parse_shebang_interpreter("#! /usr/bin/env python\n").as_deref(),
            Some("/usr/bin/env")
        );
        assert_eq!(parse_shebang_interpreter("#!/bin/sh"), Some("/bin/sh".to_owned()));
        assert_eq!(parse_shebang_interpreter("no shebang"), None);
    }
}
