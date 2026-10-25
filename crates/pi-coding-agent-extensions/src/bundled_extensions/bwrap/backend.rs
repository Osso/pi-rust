//! Port of `packages/coding-agent/extensions/bwrap/src/backend.ts`.

use crate::utils::paths::{node_dirname, node_join, node_resolve};
use indexmap::IndexMap;
use pi_js_compat::{
    js_format,
    js_json::{JsString, encode_node_utf8},
};
use std::io::Read;
use std::path::Path;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use pi_agent_core::agent::ToolError;
use pi_js_compat::abort::AbortSignal;
use tokio::io::AsyncReadExt;
use tokio::process::Command;

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
    pub bwrap_command: JsString,
    pub command: Vec<JsString>,
    pub cwd: JsString,
    pub env: Option<IndexMap<JsString, JsString>>,
    pub extra_read_only_paths: Option<Vec<JsString>>,
    pub home_dir: Option<JsString>,
    pub profile: BwrapSandboxProfile,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BwrapInvocation {
    pub argv: Vec<JsString>,
    pub command: JsString,
    pub env: IndexMap<JsString, JsString>,
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
pub fn run_bwrap_availability_check(bwrap_command: &JsString) -> Result<(), JsString> {
    let result = std::process::Command::new(encode_node_utf8(bwrap_command))
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output();
    match result {
        Err(error) => Err(js_format!(
            "bubblewrap is required for sandbox profile but is unavailable: {}",
            node_spawn_error_message("spawnSync", bwrap_command, &error)
        )),
        Ok(output) if !output.status.success() => {
            let stderr = JsString::from(String::from_utf8_lossy(&output.stderr).into_owned()).trim();
            if stderr.is_empty() {
                Err("bubblewrap is required for sandbox profile but failed to run".into())
            } else {
                Err(js_format!(
                    "bubblewrap is required for sandbox profile but failed to run: {}",
                    stderr
                ))
            }
        }
        Ok(_) => Ok(()),
    }
}

/// `buildBwrapInvocation`.
pub fn build_bwrap_invocation(options: &BwrapInvocationOptions) -> Result<BwrapInvocation, JsString> {
    let cwd = absolute_path(&options.cwd)?;
    let sandbox_env = build_sandbox_env(options.env.as_ref());
    Ok(BwrapInvocation {
        argv: build_bwrap_arguments(options, &cwd, &sandbox_env)?,
        command: options.bwrap_command.clone(),
        env: sandbox_env,
    })
}

fn build_bwrap_arguments(
    options: &BwrapInvocationOptions,
    cwd: &JsString,
    sandbox_env: &IndexMap<JsString, JsString>,
) -> Result<Vec<JsString>, JsString> {
    let mut arguments = build_bwrap_base_arguments();
    for (key, value) in sandbox_env {
        arguments.push("--setenv".into());
        arguments.push(key.into());
        arguments.push(value.into());
    }
    for system_path in READ_ONLY_SYSTEM_PATHS {
        if Path::new(system_path).exists() {
            arguments.extend(["--ro-bind", system_path, system_path].into_iter().map(JsString::from));
        }
    }
    for path in REQUIRED_SYSTEM_SYMLINKS {
        arguments.extend(build_resolved_read_only_mount_arguments(&path.into(), cwd)?);
    }
    for path in options.extra_read_only_paths.as_deref().unwrap_or_default() {
        arguments.extend(build_read_only_mount_arguments(path, cwd)?);
    }
    arguments.extend(build_workspace_mount_arguments(cwd, options.profile));
    arguments.push("--chdir".into());
    arguments.push(cwd.into());
    arguments.push("--".into());
    arguments.extend(options.command.iter().cloned());
    Ok(arguments)
}

fn build_bwrap_base_arguments() -> Vec<JsString> {
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
    .map(JsString::from)
    .collect()
}

pub struct SandboxedBashOperations {
    bwrap_command: JsString,
    profile: Option<BwrapSandboxProfile>,
    shell_path: Option<JsString>,
    local: LocalBashOperations,
}

/// `createSandboxedBashOperations`.
pub fn create_sandboxed_bash_operations(
    bwrap_command: JsString,
    profile: Option<BwrapSandboxProfile>,
    shell_path: Option<JsString>,
) -> Arc<dyn BashOperations> {
    Arc::new(SandboxedBashOperations {
        bwrap_command,
        profile,
        local: crate::core::tools::bash::create_local_bash_operations(shell_path.as_ref()),
        shell_path,
    })
}

impl BashOperations for SandboxedBashOperations {
    fn exec<'a>(
        &'a self,
        command: &'a JsString,
        cwd: &'a JsString,
        options: BashExecOptions,
    ) -> crate::core::extensions::types::BoxFuture<'a, Result<BashExecResult, ToolError>> {
        let Some(profile) = self.profile else {
            return self.local.exec(command, cwd, options);
        };
        let bwrap_command = self.bwrap_command.clone();
        let shell_path = self.shell_path.clone().unwrap_or_else(|| "/bin/sh".into());
        Box::pin(async move {
            if options.signal.as_ref().is_some_and(AbortSignal::aborted) {
                return Err(ToolError::new("aborted"));
            }
            run_bwrap_availability_check(&bwrap_command).map_err(ToolError::new)?;
            let invocation = build_bwrap_invocation(&BwrapInvocationOptions {
                bwrap_command,
                command: vec![shell_path, "-lc".into(), command.clone()],
                cwd: cwd.to_owned(),
                env: options.env.as_ref().map(|env| {
                    env.iter()
                        .filter_map(|(key, value)| value.as_ref().map(|value| (key.clone(), value.clone())))
                        .collect()
                }),
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
    let mut command = Command::new(encode_node_utf8(&invocation.command));
    command
        .args(invocation.argv.iter().map(pi_js_compat::js_json::encode_node_utf8))
        .env_clear()
        .envs(
            invocation
                .env
                .iter()
                .map(|(key, value)| (encode_node_utf8(key), encode_node_utf8(value))),
        )
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if !cfg!(windows) {
        command.process_group(0);
    }
    let mut child = command
        .spawn()
        .map_err(|error| ToolError::new(node_spawn_error_message("spawn", &invocation.command, &error)))?;
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
                            Some(signal) => { signal.wait().await; },
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
                        Some(signal) => { signal.wait().await; },
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
    if options.signal.as_ref().is_some_and(AbortSignal::aborted) {
        return Err(ToolError::new("aborted"));
    }
    if timed_out {
        return Err(ToolError::new(format!(
            "timeout:{}",
            options.timeout.unwrap_or_default()
        )));
    }
    Ok(BashExecResult {
        exit_code: status.ok().and_then(|status| status.code()).map(f64::from),
        detached: None,
    })
}

async fn forward_output(mut stream: impl tokio::io::AsyncRead + Unpin, on_data: BashDataCallback) {
    let mut buffer = [0u8; 8192];
    loop {
        match stream.read(&mut buffer).await {
            Ok(0) | Err(_) => break,
            Ok(read) => {
                on_data(&buffer[..read]).unwrap_or_else(|error| crate::utils::uncaught::throw_callback_uncaught(error))
            }
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
    host_env: &IndexMap<JsString, JsString>,
    python_path: Option<&JsString>,
) -> IndexMap<JsString, JsString> {
    let mut environment = IndexMap::new();
    if let Some(lang) = host_env.get("LANG") {
        environment.insert("LANG".into(), lang.clone());
    }
    if let Some(path) = host_env.get("PATH") {
        environment.insert("PATH".into(), path.clone());
    }
    if let Some(python_path) = python_path.filter(|value| !value.is_empty()) {
        environment.insert("PYTHONPATH".into(), python_path.to_owned());
    }
    if let Some(term) = host_env.get("TERM") {
        environment.insert("TERM".into(), term.clone());
    }
    if let Some(user) = host_env.get("USER") {
        environment.insert("USER".into(), user.clone());
    }
    environment
}

/// `createBwrapRunnerCommand`.
pub fn create_bwrap_runner_command(input: BwrapRunnerCommandOptions) -> Result<BwrapRunnerCommand, JsString> {
    run_bwrap_availability_check(&input.bwrap_command)?;
    let runner_command = find_runner_executable(&input.runner_command, input.runner_env.as_ref())?;
    let mut extra_read_only_paths = input.extra_read_only_paths.unwrap_or_default();
    extra_read_only_paths.extend(runner_read_only_paths(
        &runner_command,
        &input.runner_args,
        input.runner_env.as_ref(),
        &input.cwd,
    )?);
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
    pub bwrap_command: JsString,
    pub cwd: JsString,
    pub extra_read_only_paths: Option<Vec<JsString>>,
    pub profile: BwrapSandboxProfile,
    pub runner_args: Vec<JsString>,
    pub runner_command: JsString,
    pub runner_env: Option<IndexMap<JsString, JsString>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BwrapRunnerCommand {
    pub args: Vec<JsString>,
    pub command: JsString,
    pub env: IndexMap<JsString, JsString>,
}

fn find_runner_executable(
    command: &JsString,
    env: Option<&IndexMap<JsString, JsString>>,
) -> Result<JsString, JsString> {
    if is_absolute(command) {
        return resolve_runner_executable(command);
    }
    let search_path = env
        .and_then(|env| env.get("PATH"))
        .cloned()
        .or_else(|| pi_js_compat::process_env::get("PATH").map(JsString::from))
        .unwrap_or_else(|| DEFAULT_PATH.into());
    for directory in search_path.split(':') {
        let candidate = node_join(directory, command);
        if path_exists(&candidate) {
            return resolve_runner_executable(&candidate);
        }
    }
    Ok(command.clone())
}

fn resolve_runner_executable(path: &JsString) -> Result<JsString, JsString> {
    if path_exists(path) {
        read_canonical_path(path)
    } else {
        Ok(path.clone())
    }
}

fn read_runner_runtime_paths(command: &JsString) -> Result<Vec<JsString>, JsString> {
    let Some(interpreter) = read_absolute_shebang_interpreter(command) else {
        return Ok(Vec::new());
    };
    if !path_exists(&interpreter) {
        return Ok(Vec::new());
    }
    let interpreter_target = read_canonical_path(&interpreter)?;
    let interpreter_mount_path = if interpreter_target == interpreter {
        interpreter.clone()
    } else {
        node_dirname(&interpreter)
    };
    let virtual_environment_root = node_dirname(node_dirname(&interpreter));
    let virtual_environment_config = node_join(&virtual_environment_root, "pyvenv.cfg");
    if !path_exists(&virtual_environment_config) {
        return Ok(vec![interpreter_mount_path, interpreter_target]);
    }
    let runtime_root = node_dirname(node_dirname(&interpreter_target));
    let mut paths = vec![interpreter_mount_path, virtual_environment_config];
    for directory in PYTHON_LIBRARY_DIRECTORIES {
        paths.push(node_join(&virtual_environment_root, directory));
    }
    paths.push(interpreter_target);
    for directory in PYTHON_LIBRARY_DIRECTORIES {
        paths.push(node_join(&runtime_root, directory));
    }
    Ok(paths)
}

fn read_absolute_shebang_interpreter(path: &JsString) -> Option<JsString> {
    let mut file = std::fs::File::open(encode_node_utf8(path)).ok()?;
    let mut prefix = vec![0u8; SHEBANG_PREFIX_BYTES];
    let bytes_read = file.read(&mut prefix).ok()?;
    let prefix = JsString::from(String::from_utf8_lossy(&prefix[..bytes_read]).into_owned());
    parse_shebang_interpreter(&prefix)
}

fn parse_shebang_interpreter(prefix: &JsString) -> Option<JsString> {
    let rest = prefix.strip_prefix("#!")?;
    let start = rest.units().iter().take_while(|unit| matches!(**unit, 32 | 9)).count();
    let end = rest.units()[start..]
        .iter()
        .position(|unit| matches!(*unit, 32 | 9 | 13 | 10))
        .map_or(rest.len(), |end| start + end);
    let interpreter = rest.slice(start, end);
    interpreter.starts_with("/").then_some(interpreter)
}

fn runner_read_only_paths(
    command: &JsString,
    args: &[JsString],
    env: Option<&IndexMap<JsString, JsString>>,
    cwd: &JsString,
) -> Result<Vec<JsString>, JsString> {
    let workspace = absolute_path(cwd)?;
    let mut candidates = read_runner_runtime_paths(command)?;
    if is_absolute(command) {
        candidates.push(command.clone());
    }
    candidates.extend(args.iter().filter(|arg| is_absolute(arg)).cloned());
    if let Some(python_path) = env.and_then(|env| env.get("PYTHONPATH")) {
        candidates.extend(python_path.split(':').filter(|path| !path.is_empty()));
    }
    let mut unique = Vec::new();
    for candidate in candidates {
        let path = resolve_sandbox_path(&candidate, &workspace)?;
        if !is_sandbox_mount_unnecessary(&path, &workspace) && !unique.contains(&path) {
            unique.push(path);
        }
    }
    Ok(unique
        .iter()
        .filter(|path| {
            !unique
                .iter()
                .any(|parent| parent != *path && path.starts_with(parent.clone() + "/"))
        })
        .cloned()
        .collect())
}

fn resolve_sandbox_path(path: &JsString, workspace: &JsString) -> Result<JsString, JsString> {
    if is_absolute(path) {
        absolute_path(path)
    } else {
        node_resolve(workspace, path).map_err(|error| error.message().clone())
    }
}

fn is_sandbox_mount_unnecessary(path: &JsString, workspace: &JsString) -> bool {
    if !path_exists(path) {
        return true;
    }
    if path == workspace || path.starts_with(workspace.clone() + "/") {
        return true;
    }
    READ_ONLY_SYSTEM_PATHS
        .iter()
        .any(|system_path| path == *system_path || path.starts_with(JsString::from(*system_path) + "/"))
}

fn build_resolved_read_only_mount_arguments(path: &JsString, workspace: &JsString) -> Result<Vec<JsString>, JsString> {
    if !path_exists(path) {
        return Ok(Vec::new());
    }
    let target = read_canonical_path(path)?;
    if is_sandbox_mount_unnecessary(&target, workspace) {
        return Ok(Vec::new());
    }
    build_read_only_mount_arguments(&target, workspace)
}

fn build_read_only_mount_arguments(path: &JsString, workspace: &JsString) -> Result<Vec<JsString>, JsString> {
    let resolved_path = absolute_path(path)?;
    if !path_exists(&resolved_path) || resolved_path == *workspace {
        return Ok(Vec::new());
    }
    let mut arguments = create_parent_directory_arguments(&resolved_path);
    arguments.extend(["--ro-bind".into(), resolved_path.clone(), resolved_path]);
    Ok(arguments)
}

fn build_workspace_mount_arguments(workspace: &JsString, profile: BwrapSandboxProfile) -> Vec<JsString> {
    if !path_exists(workspace) {
        return Vec::new();
    }
    let bind_mode = if profile.0 == SandboxProfileName::WorkspaceWrite {
        "--bind"
    } else {
        "--ro-bind"
    };
    let mut arguments = create_parent_directory_arguments(workspace);
    arguments.extend([bind_mode.into(), workspace.clone(), workspace.clone()]);
    arguments
}

fn create_parent_directory_arguments(path: &JsString) -> Vec<JsString> {
    derive_parent_directories(path)
        .into_iter()
        .flat_map(|parent| ["--dir".into(), parent])
        .collect()
}

fn build_sandbox_env(env: Option<&IndexMap<JsString, JsString>>) -> IndexMap<JsString, JsString> {
    let mut sandbox_env = IndexMap::new();
    sandbox_env.insert("HOME".into(), SANDBOX_HOME.into());
    sandbox_env.insert(
        "LANG".into(),
        env.and_then(|env| env.get("LANG"))
            .cloned()
            .unwrap_or_else(|| DEFAULT_LANG.into()),
    );
    sandbox_env.insert(
        "PATH".into(),
        env.and_then(|env| env.get("PATH"))
            .cloned()
            .unwrap_or_else(|| DEFAULT_PATH.into()),
    );
    sandbox_env.insert("TMPDIR".into(), "/tmp".into());
    sandbox_env.insert("XDG_CONFIG_HOME".into(), node_join(SANDBOX_HOME, ".config"));
    if let Some(python_path) = env
        .and_then(|env| env.get("PYTHONPATH"))
        .filter(|value| !value.is_empty())
    {
        sandbox_env.insert("PYTHONPATH".into(), python_path.clone());
    }
    if let Some(term) = env.and_then(|env| env.get("TERM")).filter(|value| !value.is_empty()) {
        sandbox_env.insert("TERM".into(), term.clone());
    }
    if let Some(user) = env
        .and_then(|env| env.get("USER"))
        .cloned()
        .or_else(|| pi_js_compat::process_env::get("USER").map(JsString::from))
    {
        sandbox_env.insert("USER".into(), user);
    }
    sandbox_env
}

fn derive_parent_directories(path: &JsString) -> Vec<JsString> {
    let parts: Vec<JsString> = path.split('/').filter(|part| !part.is_empty()).collect();
    (0..parts.len().saturating_sub(1))
        .map(|index| JsString::from("/") + JsString::join(&parts[..=index], "/"))
        .collect()
}

fn is_absolute(path: &JsString) -> bool {
    path.starts_with("/")
}

fn absolute_path(path: &JsString) -> Result<JsString, JsString> {
    node_resolve("", path).map_err(|error| error.message().clone())
}

fn path_exists(path: &JsString) -> bool {
    Path::new(&encode_node_utf8(path)).exists()
}

fn read_canonical_path(path: &JsString) -> Result<JsString, JsString> {
    std::fs::canonicalize(encode_node_utf8(path))
        .map(|path| path.to_string_lossy().into_owned().into())
        .map_err(|error| pi_js_compat::node_fs::node_fs_error(&error, "lstat", path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invocation_keeps_raw_cwd_command_and_environment_until_spawn() {
        let root = tempfile::tempdir().unwrap();
        let raw = JsString::from_utf16(vec![0xd800]);
        let cwd = JsString::from(root.path().to_str().unwrap()) + "/cwd-" + &raw;
        std::fs::create_dir(pi_js_compat::js_json::encode_node_utf8(&cwd)).unwrap();
        let value = JsString::from("path-") + &raw;
        let env = IndexMap::from([("PATH".into(), value.clone())]);
        let command = JsString::from("command-") + &raw;
        let invocation = build_bwrap_invocation(&BwrapInvocationOptions {
            bwrap_command: command.clone(),
            command: vec![command.clone()],
            cwd: cwd.clone(),
            env: Some(env),
            extra_read_only_paths: None,
            home_dir: None,
            profile: BwrapSandboxProfile(SandboxProfileName::WorkspaceWrite),
        })
        .unwrap();
        assert_eq!(invocation.command, command);
        assert_eq!(invocation.argv.last(), Some(&command));
        assert!(invocation.argv.contains(&cwd));
        assert_eq!(invocation.env.get("PATH"), Some(&value));
    }

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
        assert_eq!(derive_parent_directories(&"/a/b/c".into()), vec!["/a", "/a/b"]);
        assert_eq!(derive_parent_directories(&"/a".into()), Vec::<JsString>::new());
    }

    #[test]
    fn builds_runner_environment_in_source_order() {
        let mut host = IndexMap::new();
        host.insert("LANG".into(), "C.UTF-8".into());
        host.insert("PATH".into(), "/bin".into());
        host.insert("TERM".into(), "xterm".into());
        host.insert("USER".into(), "tester".into());
        let env = create_bwrap_runner_environment(&host, Some(&"/py".into()));
        let keys: Vec<JsString> = env.keys().cloned().collect();
        assert_eq!(keys, vec!["LANG", "PATH", "PYTHONPATH", "TERM", "USER"]);
    }

    #[test]
    fn parses_shebang_interpreter() {
        assert_eq!(
            parse_shebang_interpreter(&"#!/usr/bin/python3 -u\n".into()),
            Some(JsString::from("/usr/bin/python3"))
        );
        assert_eq!(
            parse_shebang_interpreter(&"#! /usr/bin/env python\n".into()),
            Some(JsString::from("/usr/bin/env"))
        );
        assert_eq!(parse_shebang_interpreter(&"#!/bin/sh".into()), Some("/bin/sh".into()));
        assert_eq!(parse_shebang_interpreter(&"no shebang".into()), None);
    }
}
