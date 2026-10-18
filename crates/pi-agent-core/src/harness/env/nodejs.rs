//! Port of `packages/agent/src/harness/env/nodejs.ts`.
//!
//! Windows-specific shell discovery and WSL stdin transport branches are excluded; the port
//! targets the Unix runtime the harness executes on.

use std::collections::{HashMap, HashSet};
use std::io;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use pi_ai::request::BoxFuture;
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::Command;
use tokio::sync::mpsc::{self, UnboundedSender};
use tokio_util::sync::CancellationToken;

use crate::harness::types::{
    CreateDirOptions, CreateTempFileOptions, ExecutionError, ExecutionErrorCode, FileContent, FileError, FileErrorCode,
    FileInfo, FileKind, FileSystem, ReadTextLinesOptions, RemoveOptions, Shell, ShellExecOptions, ShellExecResult,
};

const MAX_TIMEOUT_MS: f64 = 2_147_483_647.0;
const MAX_TIMEOUT_SECONDS: f64 = MAX_TIMEOUT_MS / 1000.0;
const EXIT_STDIO_GRACE: Duration = Duration::from_millis(100);
static TEMP_NAME_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Options accepted by [`NodeExecutionEnv::new`].
#[derive(Debug, Clone, Default)]
pub struct NodeExecutionEnvOptions {
    pub cwd: String,
    pub shell_path: Option<String>,
    pub shell_env: HashMap<String, String>,
}

/// Filesystem and process execution environment backed by the host OS.
#[derive(Debug)]
pub struct NodeExecutionEnv {
    cwd: String,
    shell_path: Option<String>,
    shell_env: HashMap<String, String>,
    active_child_pids: Mutex<HashSet<u32>>,
}

impl NodeExecutionEnv {
    pub fn new(options: NodeExecutionEnvOptions) -> Self {
        Self {
            cwd: options.cwd,
            shell_path: options.shell_path,
            shell_env: options.shell_env,
            active_child_pids: Mutex::new(HashSet::new()),
        }
    }

    /// Terminates every shell process still tracked by this environment.
    pub async fn cleanup(&self) {
        self.kill_active_children();
    }

    fn kill_active_children(&self) {
        let pids = {
            let mut active = self.active_child_pids.lock().expect("active child pids lock");
            active.drain().collect::<Vec<_>>()
        };
        for pid in pids {
            kill_process_tree(pid);
        }
    }
}

impl FileSystem for NodeExecutionEnv {
    fn cwd(&self) -> &str {
        &self.cwd
    }

    fn absolute_path<'a>(
        &'a self,
        path: &'a str,
        _abort: Option<&'a CancellationToken>,
    ) -> BoxFuture<'a, Result<String, FileError>> {
        Box::pin(async move { Ok(resolve_path(&self.cwd, path)) })
    }

    fn join_path<'a>(
        &'a self,
        parts: &'a [String],
        _abort: Option<&'a CancellationToken>,
    ) -> BoxFuture<'a, Result<String, FileError>> {
        Box::pin(async move { Ok(join_paths(parts)) })
    }

    fn read_text_file<'a>(
        &'a self,
        path: &'a str,
        abort: Option<&'a CancellationToken>,
    ) -> BoxFuture<'a, Result<String, FileError>> {
        Box::pin(async move {
            let resolved = resolve_path(&self.cwd, path);
            if let Some(error) = abort_result(abort, &resolved) {
                return Err(error);
            }
            let bytes = tokio::fs::read(&resolved)
                .await
                .map_err(|error| file_error(&error, "open", &resolved))?;
            Ok(String::from_utf8_lossy(&bytes).into_owned())
        })
    }

    fn read_text_lines<'a>(
        &'a self,
        path: &'a str,
        options: ReadTextLinesOptions,
    ) -> BoxFuture<'a, Result<Vec<String>, FileError>> {
        Box::pin(async move {
            let resolved = resolve_path(&self.cwd, path);
            if let Some(error) = abort_result(options.abort.as_ref(), &resolved) {
                return Err(error);
            }
            if options.max_lines == Some(0) {
                return Ok(Vec::new());
            }
            let bytes = tokio::fs::read(&resolved)
                .await
                .map_err(|error| file_error(&error, "open", &resolved))?;
            if let Some(error) = abort_result(options.abort.as_ref(), &resolved) {
                return Err(error);
            }
            let content = String::from_utf8_lossy(&bytes);
            let mut lines = split_lines(&content);
            if let Some(max_lines) = options.max_lines {
                lines.truncate(max_lines);
            }
            Ok(lines)
        })
    }

    fn read_binary_file<'a>(
        &'a self,
        path: &'a str,
        abort: Option<&'a CancellationToken>,
    ) -> BoxFuture<'a, Result<Vec<u8>, FileError>> {
        Box::pin(async move {
            let resolved = resolve_path(&self.cwd, path);
            if let Some(error) = abort_result(abort, &resolved) {
                return Err(error);
            }
            tokio::fs::read(&resolved)
                .await
                .map_err(|error| file_error(&error, "open", &resolved))
        })
    }

    fn write_file<'a>(
        &'a self,
        path: &'a str,
        content: FileContent<'a>,
        abort: Option<&'a CancellationToken>,
    ) -> BoxFuture<'a, Result<(), FileError>> {
        Box::pin(async move {
            let resolved = resolve_path(&self.cwd, path);
            if let Some(error) = abort_result(abort, &resolved) {
                return Err(error);
            }
            create_parent_dir(&resolved).await?;
            if let Some(error) = abort_result(abort, &resolved) {
                return Err(error);
            }
            tokio::fs::write(&resolved, content.as_bytes())
                .await
                .map_err(|error| file_error(&error, "open", &resolved))
        })
    }

    fn append_file<'a>(&'a self, path: &'a str, content: FileContent<'a>) -> BoxFuture<'a, Result<(), FileError>> {
        Box::pin(async move {
            let resolved = resolve_path(&self.cwd, path);
            create_parent_dir(&resolved).await?;
            use tokio::io::AsyncWriteExt;
            let mut file = tokio::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&resolved)
                .await
                .map_err(|error| file_error(&error, "open", &resolved))?;
            file.write_all(content.as_bytes())
                .await
                .map_err(|error| file_error(&error, "write", &resolved))?;
            file.flush()
                .await
                .map_err(|error| file_error(&error, "write", &resolved))
        })
    }

    fn rename_file<'a>(
        &'a self,
        source_path: &'a str,
        destination_path: &'a str,
        abort: Option<&'a CancellationToken>,
    ) -> BoxFuture<'a, Result<(), FileError>> {
        Box::pin(async move {
            let source = resolve_path(&self.cwd, source_path);
            let destination = resolve_path(&self.cwd, destination_path);
            if let Some(error) = abort_result(abort, &destination) {
                return Err(error);
            }
            tokio::fs::rename(&source, &destination)
                .await
                .map_err(|error| rename_error(&error, &source, &destination))
        })
    }

    fn file_info<'a>(&'a self, path: &'a str) -> BoxFuture<'a, Result<FileInfo, FileError>> {
        Box::pin(async move {
            let resolved = resolve_path(&self.cwd, path);
            let metadata = tokio::fs::symlink_metadata(&resolved)
                .await
                .map_err(|error| file_error(&error, "lstat", &resolved))?;
            file_info_from_metadata(&resolved, &metadata)
        })
    }

    fn list_dir<'a>(
        &'a self,
        path: &'a str,
        abort: Option<&'a CancellationToken>,
    ) -> BoxFuture<'a, Result<Vec<FileInfo>, FileError>> {
        Box::pin(async move {
            let resolved = resolve_path(&self.cwd, path);
            if let Some(error) = abort_result(abort, &resolved) {
                return Err(error);
            }
            let mut entries = tokio::fs::read_dir(&resolved)
                .await
                .map_err(|error| file_error(&error, "scandir", &resolved))?;
            let mut infos = Vec::new();
            while let Some(entry) = entries
                .next_entry()
                .await
                .map_err(|error| file_error(&error, "scandir", &resolved))?
            {
                if let Some(error) = abort_result(abort, &resolved) {
                    return Err(error);
                }
                let entry_path = entry.path().to_string_lossy().into_owned();
                let metadata = tokio::fs::symlink_metadata(&entry_path)
                    .await
                    .map_err(|error| file_error(&error, "lstat", &entry_path))?;
                if let Ok(info) = file_info_from_metadata(&entry_path, &metadata) {
                    infos.push(info);
                }
            }
            Ok(infos)
        })
    }

    fn canonical_path<'a>(&'a self, path: &'a str) -> BoxFuture<'a, Result<String, FileError>> {
        Box::pin(async move {
            let resolved = resolve_path(&self.cwd, path);
            tokio::fs::canonicalize(&resolved)
                .await
                .map(|path| path.to_string_lossy().into_owned())
                .map_err(|error| file_error(&error, "realpath", &resolved))
        })
    }

    fn exists<'a>(
        &'a self,
        path: &'a str,
        _abort: Option<&'a CancellationToken>,
    ) -> BoxFuture<'a, Result<bool, FileError>> {
        Box::pin(async move {
            let resolved = resolve_path(&self.cwd, path);
            match tokio::fs::symlink_metadata(&resolved).await {
                Ok(_) => Ok(true),
                Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
                Err(error) => Err(file_error(&error, "lstat", &resolved)),
            }
        })
    }

    fn create_dir<'a>(&'a self, path: &'a str, options: CreateDirOptions) -> BoxFuture<'a, Result<(), FileError>> {
        Box::pin(async move {
            let resolved = resolve_path(&self.cwd, path);
            let result = if options.recursive.unwrap_or(true) {
                tokio::fs::create_dir_all(&resolved).await
            } else {
                tokio::fs::create_dir(&resolved).await
            };
            result.map_err(|error| file_error(&error, "mkdir", &resolved))
        })
    }

    fn remove<'a>(&'a self, path: &'a str, options: RemoveOptions) -> BoxFuture<'a, Result<(), FileError>> {
        Box::pin(async move {
            let resolved = resolve_path(&self.cwd, path);
            remove_path(&resolved, options).await
        })
    }

    fn create_temp_dir<'a>(&'a self, prefix: &'a str) -> BoxFuture<'a, Result<String, FileError>> {
        Box::pin(async move { create_temp_dir(prefix).await })
    }

    fn create_temp_file<'a>(&'a self, options: CreateTempFileOptions) -> BoxFuture<'a, Result<String, FileError>> {
        Box::pin(async move {
            let directory = create_temp_dir("tmp-").await?;
            let name = format!(
                "{}{}{}",
                options.prefix.unwrap_or_default(),
                random_name(),
                options.suffix.unwrap_or_default()
            );
            let file_path = Path::new(&directory).join(name).to_string_lossy().into_owned();
            tokio::fs::write(&file_path, b"")
                .await
                .map_err(|error| file_error(&error, "open", &file_path))?;
            Ok(file_path)
        })
    }

    fn cleanup(&self) -> BoxFuture<'_, ()> {
        Box::pin(async move { self.kill_active_children() })
    }
}

impl Shell for NodeExecutionEnv {
    fn exec<'a>(
        &'a self,
        command: &'a str,
        options: ShellExecOptions,
    ) -> BoxFuture<'a, Result<ShellExecResult, ExecutionError>> {
        Box::pin(async move { self.exec_command(command, options).await })
    }

    fn cleanup(&self) -> BoxFuture<'_, ()> {
        Box::pin(async move { self.kill_active_children() })
    }
}

impl NodeExecutionEnv {
    async fn exec_command(&self, command: &str, options: ShellExecOptions) -> Result<ShellExecResult, ExecutionError> {
        if options.abort.as_ref().is_some_and(CancellationToken::is_cancelled) {
            return Err(ExecutionError::new(ExecutionErrorCode::Aborted, "aborted", None));
        }
        let timeout_ms = resolve_timeout_ms(options.timeout)?;
        let cwd = match &options.cwd {
            Some(path) => resolve_path(&self.cwd, path),
            None => self.cwd.clone(),
        };
        let shell = get_shell_config(self.shell_path.as_deref()).await?;
        if !path_exists(&cwd).await {
            return Err(ExecutionError::new(
                ExecutionErrorCode::SpawnError,
                format!("Working directory does not exist: {cwd}\nCannot execute bash commands."),
                None,
            ));
        }

        let mut process = Command::new(&shell.shell);
        process.args(&shell.args).arg(command);
        process.current_dir(&cwd);
        process.env_clear();
        process.envs(build_env(&self.shell_env, options.env.as_ref(), options.inherit_env));
        process.stdin(std::process::Stdio::null());
        process.stdout(std::process::Stdio::piped());
        process.stderr(std::process::Stdio::piped());
        #[cfg(unix)]
        process.process_group(0);

        let mut child = process.spawn().map_err(|error| {
            ExecutionError::new(ExecutionErrorCode::SpawnError, error.to_string(), Some(Box::new(error)))
        })?;
        let pid = child.id();
        if let Some(pid) = pid {
            self.active_child_pids
                .lock()
                .expect("active child pids lock")
                .insert(pid);
        }

        let stdout = child.stdout.take();
        let stderr = child.stderr.take();
        let (event_sender, mut event_receiver) = mpsc::unbounded_channel();
        let stdout_text = Arc::new(Mutex::new(String::new()));
        let stderr_text = Arc::new(Mutex::new(String::new()));
        let stdout_task = stdout.map(|reader| {
            tokio::spawn(read_stream(
                reader,
                StreamKind::Stdout,
                Arc::clone(&stdout_text),
                options.on_stdout.clone(),
                event_sender.clone(),
            ))
        });
        let stderr_task = stderr.map(|reader| {
            tokio::spawn(read_stream(
                reader,
                StreamKind::Stderr,
                Arc::clone(&stderr_text),
                options.on_stderr.clone(),
                event_sender.clone(),
            ))
        });

        let deadline = timeout_ms.map(|millis| tokio::time::Instant::now() + Duration::from_secs_f64(millis / 1000.0));
        let abort = options.abort.clone();
        let mut timed_out = false;
        let mut aborted = false;
        let mut done = [false, false];
        let mut callback_error = None;
        let mut killed_for_callback = false;

        let status = loop {
            tokio::select! {
                status = child.wait() => break status,
                _ = async {
                    match deadline {
                        Some(deadline) => tokio::time::sleep_until(deadline).await,
                        None => std::future::pending::<()>().await,
                    }
                }, if !timed_out => {
                    timed_out = true;
                    if let Some(pid) = pid {
                        kill_process_tree(pid);
                    }
                }
                _ = async {
                    match &abort {
                        Some(token) => token.cancelled().await,
                        None => std::future::pending::<()>().await,
                    }
                }, if !aborted => {
                    aborted = true;
                    if let Some(pid) = pid {
                        kill_process_tree(pid);
                    }
                }
                event = event_receiver.recv() => {
                    apply_stream_event(event, &mut done, &mut callback_error);
                    if callback_error.is_some() && !killed_for_callback {
                        killed_for_callback = true;
                        if let Some(pid) = pid {
                            kill_process_tree(pid);
                        }
                    }
                }
            }
        };

        let grace = tokio::time::sleep(EXIT_STDIO_GRACE);
        tokio::pin!(grace);
        while !(done[0] && done[1]) {
            tokio::select! {
                _ = &mut grace => break,
                event = event_receiver.recv() => {
                    if apply_stream_event(event, &mut done, &mut callback_error) {
                        break;
                    }
                }
            }
        }
        if let Some(task) = stdout_task {
            task.abort();
        }
        if let Some(task) = stderr_task {
            task.abort();
        }
        if let Some(pid) = pid {
            self.active_child_pids
                .lock()
                .expect("active child pids lock")
                .remove(&pid);
        }

        if let Some(message) = callback_error {
            return Err(ExecutionError::new(ExecutionErrorCode::CallbackError, message, None));
        }
        if timed_out {
            return Err(ExecutionError::new(
                ExecutionErrorCode::Timeout,
                format!("timeout:{}", format_js_number(options.timeout.unwrap_or_default())),
                None,
            ));
        }
        if aborted || options.abort.as_ref().is_some_and(CancellationToken::is_cancelled) {
            return Err(ExecutionError::new(ExecutionErrorCode::Aborted, "aborted", None));
        }

        Ok(ShellExecResult {
            stdout: stdout_text.lock().expect("stdout lock").clone(),
            stderr: stderr_text.lock().expect("stderr lock").clone(),
            exit_code: status.map(|status| status.code().unwrap_or(0)).unwrap_or(0),
        })
    }
}

#[derive(Debug, Clone, Copy)]
enum StreamKind {
    Stdout,
    Stderr,
}

struct StreamEvent {
    kind: StreamKind,
    result: Result<(), String>,
}

fn apply_stream_event(event: Option<StreamEvent>, done: &mut [bool; 2], callback_error: &mut Option<String>) -> bool {
    let Some(event) = event else {
        return true;
    };
    done[match event.kind {
        StreamKind::Stdout => 0,
        StreamKind::Stderr => 1,
    }] = true;
    if let Err(message) = event.result {
        *callback_error = Some(message);
    }
    false
}

async fn read_stream<R>(
    mut reader: R,
    kind: StreamKind,
    shared: Arc<Mutex<String>>,
    callback: Option<Arc<dyn Fn(&str) -> Result<(), String> + Send + Sync>>,
    sender: UnboundedSender<StreamEvent>,
) where
    R: AsyncRead + Unpin,
{
    let mut decoder = Utf8StreamDecoder::default();
    let mut buffer = [0u8; 8192];
    loop {
        match reader.read(&mut buffer).await {
            Ok(0) => break,
            Ok(read) => {
                let text = decoder.push(&buffer[..read]);
                if text.is_empty() {
                    continue;
                }
                shared.lock().expect("stream text lock").push_str(&text);
                if let Some(callback) = &callback {
                    if let Err(message) = callback(&text) {
                        let _ = sender.send(StreamEvent {
                            kind,
                            result: Err(message),
                        });
                        return;
                    }
                }
            }
            Err(error) => {
                let _ = sender.send(StreamEvent {
                    kind,
                    result: Err(error.to_string()),
                });
                return;
            }
        }
    }
    let tail = decoder.finish();
    if !tail.is_empty() {
        shared.lock().expect("stream text lock").push_str(&tail);
        if let Some(callback) = &callback {
            if let Err(message) = callback(&tail) {
                let _ = sender.send(StreamEvent {
                    kind,
                    result: Err(message),
                });
                return;
            }
        }
    }
    let _ = sender.send(StreamEvent { kind, result: Ok(()) });
}

#[derive(Default)]
struct Utf8StreamDecoder {
    pending: Vec<u8>,
}

impl Utf8StreamDecoder {
    fn push(&mut self, bytes: &[u8]) -> String {
        self.pending.extend_from_slice(bytes);
        let mut output = String::new();
        loop {
            match std::str::from_utf8(&self.pending) {
                Ok(text) => {
                    output.push_str(text);
                    self.pending.clear();
                    break;
                }
                Err(error) => {
                    let valid = error.valid_up_to();
                    output.push_str(std::str::from_utf8(&self.pending[..valid]).expect("valid UTF-8 prefix"));
                    match error.error_len() {
                        Some(length) => {
                            output.push('\u{fffd}');
                            self.pending.drain(..valid + length);
                        }
                        None => {
                            self.pending.drain(..valid);
                            break;
                        }
                    }
                }
            }
        }
        output
    }

    fn finish(&mut self) -> String {
        let output = String::from_utf8_lossy(&self.pending).into_owned();
        self.pending.clear();
        output
    }
}

fn resolve_timeout_ms(timeout: Option<f64>) -> Result<Option<f64>, ExecutionError> {
    let Some(timeout) = timeout else {
        return Ok(None);
    };
    if !timeout.is_finite() || timeout <= 0.0 {
        return Err(ExecutionError::new(
            ExecutionErrorCode::Timeout,
            "Invalid timeout: must be a finite number of seconds",
            None,
        ));
    }
    let timeout_ms = timeout * 1000.0;
    if timeout_ms > MAX_TIMEOUT_MS {
        return Err(ExecutionError::new(
            ExecutionErrorCode::Timeout,
            format!(
                "Invalid timeout: maximum is {} seconds",
                format_js_number(MAX_TIMEOUT_SECONDS)
            ),
            None,
        ));
    }
    Ok(Some(timeout_ms))
}

fn format_js_number(value: f64) -> String {
    if value.fract() == 0.0 && value.abs() < 1e21 {
        return format!("{}", value as i64);
    }
    format!("{value}")
}

fn build_env(
    shell_env: &HashMap<String, String>,
    extra_env: Option<&HashMap<String, String>>,
    inherit_env: Option<bool>,
) -> HashMap<String, String> {
    if inherit_env == Some(false) {
        return extra_env.cloned().unwrap_or_default();
    }
    let mut environment = std::env::vars_os()
        .map(|(key, value)| (key.to_string_lossy().into_owned(), value.to_string_lossy().into_owned()))
        .collect::<HashMap<_, _>>();
    environment.extend(shell_env.iter().map(|(key, value)| (key.clone(), value.clone())));
    if let Some(extra_env) = extra_env {
        environment.extend(extra_env.iter().map(|(key, value)| (key.clone(), value.clone())));
    }
    environment
}

struct ShellConfig {
    shell: String,
    args: Vec<String>,
}

async fn get_shell_config(custom_shell_path: Option<&str>) -> Result<ShellConfig, ExecutionError> {
    if let Some(custom_shell_path) = custom_shell_path {
        if path_exists(custom_shell_path).await {
            return Ok(ShellConfig {
                shell: custom_shell_path.to_owned(),
                args: vec!["-c".to_owned()],
            });
        }
        return Err(ExecutionError::new(
            ExecutionErrorCode::ShellUnavailable,
            format!("Custom shell path not found: {custom_shell_path}"),
            None,
        ));
    }
    if path_exists("/bin/bash").await {
        return Ok(ShellConfig {
            shell: "/bin/bash".to_owned(),
            args: vec!["-c".to_owned()],
        });
    }
    if let Some(bash) = find_bash_on_path().await {
        return Ok(ShellConfig {
            shell: bash,
            args: vec!["-c".to_owned()],
        });
    }
    Ok(ShellConfig {
        shell: "sh".to_owned(),
        args: vec!["-c".to_owned()],
    })
}

async fn find_bash_on_path() -> Option<String> {
    let (stdout, status) = run_command("which", &["bash"], 5000).await?;
    if status != 0 || stdout.is_empty() {
        return None;
    }
    let first_match = stdout.trim().lines().next()?.trim();
    if first_match.is_empty() || !path_exists(first_match).await {
        return None;
    }
    Some(first_match.to_owned())
}

async fn run_command(command: &str, args: &[&str], timeout_ms: u64) -> Option<(String, i32)> {
    let child = Command::new(command)
        .args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .ok()?;
    let pid = child.id();
    let wait = child.wait_with_output();
    tokio::pin!(wait);
    let deadline = tokio::time::sleep(Duration::from_millis(timeout_ms));
    tokio::pin!(deadline);
    let output = tokio::select! {
        output = &mut wait => output.ok()?,
        _ = &mut deadline => {
            if let Some(pid) = pid {
                kill_process_tree(pid);
            }
            wait.await.ok()?
        }
    };
    Some((
        String::from_utf8_lossy(&output.stdout).into_owned(),
        output.status.code().unwrap_or(-1),
    ))
}

async fn path_exists(path: &str) -> bool {
    tokio::fs::metadata(path).await.is_ok()
}

fn kill_process_tree(pid: u32) {
    let Some(pid) = rustix::process::Pid::from_raw(pid as i32) else {
        return;
    };
    let _ = rustix::process::kill_process_group(pid, rustix::process::Signal::KILL);
}

async fn create_parent_dir(resolved: &str) -> Result<(), FileError> {
    let parent = Path::new(resolved)
        .parent()
        .map(|parent| parent.to_string_lossy().into_owned())
        .unwrap_or_else(|| resolved.to_owned());
    tokio::fs::create_dir_all(&parent)
        .await
        .map_err(|error| file_error(&error, "mkdir", &parent))
}

async fn remove_path(resolved: &str, options: RemoveOptions) -> Result<(), FileError> {
    let metadata = match tokio::fs::symlink_metadata(resolved).await {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound && options.force.unwrap_or(false) => return Ok(()),
        Err(error) => return Err(file_error(&error, "lstat", resolved)),
    };
    if metadata.is_dir() && !metadata.file_type().is_symlink() {
        if !options.recursive.unwrap_or(false) {
            return Err(FileError::new(
                FileErrorCode::Unknown,
                format!("Path is a directory: rm returned EISDIR (is a directory) {resolved}"),
                Some(resolved.to_owned()),
                None,
            ));
        }
        tokio::fs::remove_dir_all(resolved)
            .await
            .map_err(|error| file_error(&error, "rmdir", resolved))
    } else {
        tokio::fs::remove_file(resolved)
            .await
            .map_err(|error| file_error(&error, "unlink", resolved))
    }
}

async fn create_temp_dir(prefix: &str) -> Result<String, FileError> {
    let base = std::env::temp_dir();
    for _ in 0..100 {
        let candidate = base.join(format!("{prefix}{}", random_name_suffix()));
        match tokio::fs::create_dir(&candidate).await {
            Ok(()) => return Ok(candidate.to_string_lossy().into_owned()),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(file_error(&error, "mkdir", &candidate.to_string_lossy())),
        }
    }
    Err(FileError::new(
        FileErrorCode::Unknown,
        "Could not create a unique temporary directory".to_owned(),
        None,
        None,
    ))
}

fn random_name() -> String {
    let counter = TEMP_NAME_COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    format!("{nanos:032x}{counter:016x}")
}

fn random_name_suffix() -> String {
    let counter = TEMP_NAME_COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    format!("{:06x}", (nanos as u64 ^ counter) & 0xff_ffff)
}

fn split_lines(content: &str) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();
    let mut characters = content.chars().peekable();
    while let Some(character) = characters.next() {
        match character {
            '\n' => {
                lines.push(std::mem::take(&mut current));
            }
            '\r' => {
                if characters.peek() == Some(&'\n') {
                    characters.next();
                }
                lines.push(std::mem::take(&mut current));
            }
            other => current.push(other),
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    lines
}

fn abort_result(abort: Option<&CancellationToken>, path: &str) -> Option<FileError> {
    abort
        .filter(|token| token.is_cancelled())
        .map(|_| FileError::new(FileErrorCode::Aborted, "aborted", Some(path.to_owned()), None))
}

fn file_info_from_metadata(path: &str, metadata: &std::fs::Metadata) -> Result<FileInfo, FileError> {
    let file_type = metadata.file_type();
    let kind = if file_type.is_file() {
        FileKind::File
    } else if file_type.is_dir() {
        FileKind::Directory
    } else if file_type.is_symlink() {
        FileKind::Symlink
    } else {
        return Err(FileError::new(
            FileErrorCode::Invalid,
            "Unsupported file type",
            Some(path.to_owned()),
            None,
        ));
    };
    Ok(FileInfo {
        name: Path::new(path)
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default(),
        path: path.to_owned(),
        kind,
        size: metadata.len() as f64,
        mtime_ms: metadata
            .modified()
            .ok()
            .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
            .map(|duration| duration.as_secs_f64() * 1000.0)
            .unwrap_or(0.0),
    })
}

fn file_error(error: &io::Error, syscall: &str, path: &str) -> FileError {
    let (code, description, error_code) = match error_parts(error) {
        Some(parts) => parts,
        None => return unknown_file_error(error, syscall, path),
    };
    FileError::new(
        error_code,
        format!("{code}: {description}, {syscall} '{path}'"),
        Some(path.to_owned()),
        Some(Box::new(io::Error::new(error.kind(), error.to_string()))),
    )
}

fn rename_error(error: &io::Error, source: &str, destination: &str) -> FileError {
    let (code, description, error_code) = match error_parts(error) {
        Some(parts) => parts,
        None => return unknown_file_error(error, "rename", source),
    };
    FileError::new(
        error_code,
        format!("{code}: {description}, rename '{source}' -> '{destination}'"),
        Some(source.to_owned()),
        Some(Box::new(io::Error::new(error.kind(), error.to_string()))),
    )
}

fn error_parts(error: &io::Error) -> Option<(&'static str, &'static str, FileErrorCode)> {
    Some(match error.kind() {
        io::ErrorKind::NotFound => ("ENOENT", "no such file or directory", FileErrorCode::NotFound),
        io::ErrorKind::PermissionDenied => ("EACCES", "permission denied", FileErrorCode::PermissionDenied),
        io::ErrorKind::NotADirectory => ("ENOTDIR", "not a directory", FileErrorCode::NotDirectory),
        io::ErrorKind::IsADirectory => ("EISDIR", "illegal operation on a directory", FileErrorCode::IsDirectory),
        io::ErrorKind::InvalidInput => ("EINVAL", "invalid argument", FileErrorCode::Invalid),
        io::ErrorKind::AlreadyExists => ("EEXIST", "file already exists", FileErrorCode::Unknown),
        io::ErrorKind::DirectoryNotEmpty => ("ENOTEMPTY", "directory not empty", FileErrorCode::Unknown),
        _ => return None,
    })
}

fn unknown_file_error(error: &io::Error, syscall: &str, path: &str) -> FileError {
    FileError::new(
        FileErrorCode::Unknown,
        format!("UNKNOWN: {error}, {syscall} '{path}'"),
        Some(path.to_owned()),
        Some(Box::new(io::Error::new(error.kind(), error.to_string()))),
    )
}

fn resolve_path(cwd: &str, path: &str) -> String {
    let normalized = expand_path(path);
    let absolute = if normalized.starts_with('/') {
        normalized
    } else {
        let cwd = if cwd.starts_with('/') {
            cwd.to_owned()
        } else {
            std::env::current_dir()
                .map(|current| current.to_string_lossy().into_owned())
                .unwrap_or_else(|_| "/".to_owned())
        };
        format!("{cwd}/{normalized}")
    };
    node_normalize(&absolute)
}

fn expand_path(path: &str) -> String {
    if path == "~" {
        return home_dir();
    }
    if let Some(rest) = path.strip_prefix("~/") {
        return format!("{}/{}", home_dir(), rest);
    }
    if path.starts_with("file://") {
        if let Some(decoded) = file_url_to_path(path) {
            return decoded;
        }
    }
    path.to_owned()
}

fn home_dir() -> String {
    std::env::var("HOME").unwrap_or_else(|_| "/".to_owned())
}

fn file_url_to_path(url: &str) -> Option<String> {
    let rest = url.strip_prefix("file://")?;
    let path = if let Some(path) = rest.strip_prefix("localhost/") {
        format!("/{path}")
    } else if rest.starts_with('/') {
        rest.to_owned()
    } else {
        return None;
    };
    Some(percent_decode(&path))
}

fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let high = hex_value(bytes[index + 1]);
            let low = hex_value(bytes[index + 2]);
            if let (Some(high), Some(low)) = (high, low) {
                output.push(high << 4 | low);
                index += 3;
                continue;
            }
        }
        output.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&output).into_owned()
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn join_paths(parts: &[String]) -> String {
    let joined = parts
        .iter()
        .filter(|part| !part.is_empty())
        .cloned()
        .collect::<Vec<_>>()
        .join("/");
    if joined.is_empty() {
        ".".to_owned()
    } else {
        node_normalize(&joined)
    }
}

fn node_normalize(path: &str) -> String {
    let absolute = path.starts_with('/');
    let mut components: Vec<&str> = Vec::new();
    for component in path.split('/') {
        match component {
            "" | "." => {}
            ".." => {
                if components.last().is_some_and(|last| *last != "..") {
                    components.pop();
                } else if !absolute {
                    components.push("..");
                }
            }
            other => components.push(other),
        }
    }
    let joined = components.join("/");
    if absolute {
        format!("/{joined}")
    } else if joined.is_empty() {
        ".".to_owned()
    } else {
        joined
    }
}
