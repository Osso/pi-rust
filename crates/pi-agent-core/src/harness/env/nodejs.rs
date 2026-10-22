//! Port of `packages/agent/src/harness/env/nodejs.ts`.
//!
//! Windows-specific shell discovery and WSL stdin transport branches are excluded; the port
//! targets the Unix runtime the harness executes on.

use std::collections::{HashMap, HashSet};
use std::io;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use pi_ai::request::BoxFuture;
use pi_js_compat::js_format;
use pi_js_compat::js_json::{JsString, encode_node_utf8};
use pi_js_compat::number::js_number_to_string;
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::Command;
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};
use tokio_util::sync::CancellationToken;

use crate::harness::types::{
    CreateDirOptions, CreateTempFileOptions, ExecutionError, ExecutionErrorCode, FileContent, FileError, FileErrorCode,
    FileInfo, FileKind, FileSystem, ReadTextLinesOptions, RemoveOptions, Shell, ShellExecOptions, ShellExecResult,
    ShellOutputHandler,
};

const MAX_TIMEOUT_MS: f64 = 2_147_483_647.0;
const MAX_TIMEOUT_SECONDS: f64 = MAX_TIMEOUT_MS / 1000.0;
const EXIT_STDIO_GRACE: Duration = Duration::from_millis(100);
static TEMP_NAME_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Options accepted by [`NodeExecutionEnv::new`].
#[derive(Debug, Clone, Default)]
pub struct NodeExecutionEnvOptions {
    pub cwd: JsString,
    pub shell_path: Option<String>,
    pub shell_env: HashMap<String, String>,
}

/// Filesystem and process execution environment backed by the host OS.
#[derive(Debug)]
pub struct NodeExecutionEnv {
    cwd: JsString,
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
    fn cwd(&self) -> &JsString {
        &self.cwd
    }

    fn absolute_path<'a>(
        &'a self,
        path: &'a JsString,
        _abort: Option<&'a CancellationToken>,
    ) -> BoxFuture<'a, Result<JsString, FileError>> {
        Box::pin(async move { Ok(resolve_path(&self.cwd, path)) })
    }

    fn join_path<'a>(
        &'a self,
        parts: &'a [JsString],
        _abort: Option<&'a CancellationToken>,
    ) -> BoxFuture<'a, Result<JsString, FileError>> {
        Box::pin(async move { Ok(join_paths(parts)) })
    }

    fn read_text_file<'a>(
        &'a self,
        path: &'a JsString,
        abort: Option<&'a CancellationToken>,
    ) -> BoxFuture<'a, Result<String, FileError>> {
        Box::pin(async move {
            let resolved = resolve_path(&self.cwd, path);
            if let Some(error) = abort_result(abort, &resolved) {
                return Err(error);
            }
            let bytes = tokio::fs::read(&encode_node_utf8(&resolved))
                .await
                .map_err(|error| file_error(&error, "open", &resolved))?;
            Ok(String::from_utf8_lossy(&bytes).into_owned())
        })
    }

    fn read_text_lines<'a>(
        &'a self,
        path: &'a JsString,
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
            let bytes = tokio::fs::read(&encode_node_utf8(&resolved))
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
        path: &'a JsString,
        abort: Option<&'a CancellationToken>,
    ) -> BoxFuture<'a, Result<Vec<u8>, FileError>> {
        Box::pin(async move {
            let resolved = resolve_path(&self.cwd, path);
            if let Some(error) = abort_result(abort, &resolved) {
                return Err(error);
            }
            tokio::fs::read(&encode_node_utf8(&resolved))
                .await
                .map_err(|error| file_error(&error, "open", &resolved))
        })
    }

    fn write_file<'a>(
        &'a self,
        path: &'a JsString,
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
            tokio::fs::write(&encode_node_utf8(&resolved), content.as_bytes())
                .await
                .map_err(|error| file_error(&error, "open", &resolved))
        })
    }

    fn append_file<'a>(
        &'a self,
        path: &'a JsString,
        content: FileContent<'a>,
        _abort: Option<&'a CancellationToken>,
    ) -> BoxFuture<'a, Result<(), FileError>> {
        Box::pin(async move {
            let resolved = resolve_path(&self.cwd, path);
            create_parent_dir(&resolved).await?;
            use tokio::io::AsyncWriteExt;
            let mut file = tokio::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&encode_node_utf8(&resolved))
                .await
                .map_err(|error| file_error(&error, "open", &resolved))?;
            file.write_all(&content.as_bytes())
                .await
                .map_err(|error| file_error(&error, "write", &resolved))?;
            file.flush()
                .await
                .map_err(|error| file_error(&error, "write", &resolved))
        })
    }

    fn rename_file<'a>(
        &'a self,
        source_path: &'a JsString,
        destination_path: &'a JsString,
        abort: Option<&'a CancellationToken>,
    ) -> BoxFuture<'a, Result<(), FileError>> {
        Box::pin(async move {
            let source = resolve_path(&self.cwd, source_path);
            let destination = resolve_path(&self.cwd, destination_path);
            if let Some(error) = abort_result(abort, &destination) {
                return Err(error);
            }
            tokio::fs::rename(&encode_node_utf8(&source), &encode_node_utf8(&destination))
                .await
                .map_err(|error| rename_error(&error, &source, &destination))
        })
    }

    fn file_info<'a>(&'a self, path: &'a JsString) -> BoxFuture<'a, Result<FileInfo, FileError>> {
        Box::pin(async move {
            let resolved = resolve_path(&self.cwd, path);
            let metadata = tokio::fs::symlink_metadata(&encode_node_utf8(&resolved))
                .await
                .map_err(|error| file_error(&error, "lstat", &resolved))?;
            file_info_from_metadata(&resolved, &metadata)
        })
    }

    fn list_dir<'a>(
        &'a self,
        path: &'a JsString,
        abort: Option<&'a CancellationToken>,
    ) -> BoxFuture<'a, Result<Vec<FileInfo>, FileError>> {
        Box::pin(async move {
            let resolved = resolve_path(&self.cwd, path);
            if let Some(error) = abort_result(abort, &resolved) {
                return Err(error);
            }
            let entries = read_sorted_directory(&resolved).await?;
            let mut infos = Vec::new();
            for entry in entries {
                if let Some(error) = abort_result(abort, &resolved) {
                    return Err(error);
                }
                let entry_path = join_paths(&[
                    resolved.clone(),
                    JsString::from(entry.file_name().to_string_lossy().into_owned()),
                ]);
                let metadata = tokio::fs::symlink_metadata(&encode_node_utf8(&entry_path))
                    .await
                    .map_err(|error| file_error(&error, "lstat", &entry_path))?;
                if let Ok(info) = file_info_from_metadata(&entry_path, &metadata) {
                    infos.push(info);
                }
            }
            Ok(infos)
        })
    }

    fn canonical_path<'a>(&'a self, path: &'a JsString) -> BoxFuture<'a, Result<JsString, FileError>> {
        Box::pin(async move {
            let resolved = resolve_path(&self.cwd, path);
            tokio::fs::canonicalize(&encode_node_utf8(&resolved))
                .await
                .map(|path| JsString::from(path.to_string_lossy().into_owned()))
                .map_err(|error| file_error(&error, "realpath", &resolved))
        })
    }

    fn exists<'a>(
        &'a self,
        path: &'a JsString,
        _abort: Option<&'a CancellationToken>,
    ) -> BoxFuture<'a, Result<bool, FileError>> {
        Box::pin(async move {
            let resolved = resolve_path(&self.cwd, path);
            match tokio::fs::symlink_metadata(&encode_node_utf8(&resolved)).await {
                Ok(_) => Ok(true),
                Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
                Err(error) => Err(file_error(&error, "lstat", &resolved)),
            }
        })
    }

    fn create_dir<'a>(&'a self, path: &'a JsString, options: CreateDirOptions) -> BoxFuture<'a, Result<(), FileError>> {
        Box::pin(async move {
            let resolved = resolve_path(&self.cwd, path);
            let result = if options.recursive.unwrap_or(true) {
                tokio::fs::create_dir_all(&encode_node_utf8(&resolved)).await
            } else {
                tokio::fs::create_dir(&encode_node_utf8(&resolved)).await
            };
            result.map_err(|error| file_error(&error, "mkdir", &resolved))
        })
    }

    fn remove<'a>(&'a self, path: &'a JsString, options: RemoveOptions) -> BoxFuture<'a, Result<(), FileError>> {
        Box::pin(async move {
            let resolved = resolve_path(&self.cwd, path);
            remove_path(&resolved, options).await
        })
    }

    fn create_temp_dir<'a>(&'a self, prefix: &'a JsString) -> BoxFuture<'a, Result<JsString, FileError>> {
        Box::pin(async move { create_temp_dir(prefix).await })
    }

    fn create_temp_file<'a>(&'a self, options: CreateTempFileOptions) -> BoxFuture<'a, Result<JsString, FileError>> {
        Box::pin(async move {
            let directory = create_temp_dir(&"tmp-".into()).await?;
            let name = js_format!(
                "{}{}{}",
                options.prefix.unwrap_or_default(),
                random_name(),
                options.suffix.unwrap_or_default()
            );
            let file_path = join_paths(&[directory, name]);
            tokio::fs::write(&encode_node_utf8(&file_path), b"")
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
        command: &'a JsString,
        options: ShellExecOptions,
    ) -> BoxFuture<'a, Result<ShellExecResult, ExecutionError>> {
        Box::pin(async move { self.exec_command(command, options).await })
    }

    fn cleanup(&self) -> BoxFuture<'_, ()> {
        Box::pin(async move { self.kill_active_children() })
    }
}

impl NodeExecutionEnv {
    async fn exec_command(
        &self,
        command: &JsString,
        options: ShellExecOptions,
    ) -> Result<ShellExecResult, ExecutionError> {
        if options.abort.as_ref().is_some_and(CancellationToken::is_cancelled) {
            return Err(ExecutionError::new(ExecutionErrorCode::Aborted, "aborted", None));
        }
        let timeout_ms = resolve_timeout_ms(options.timeout)?;
        let cwd = resolve_exec_cwd(&self.cwd, options.cwd.as_ref());
        let shell = get_shell_config(self.shell_path.as_deref()).await?;
        ensure_working_directory(&cwd).await?;

        let mut child = self.spawn_shell(&shell, command, &cwd, &options)?;
        let pid = child.id();
        self.track_child(pid);

        let mut tasks = start_stream_tasks(&mut child, options.on_stdout.clone(), options.on_stderr.clone());
        let wait = wait_for_exit(&mut child, pid, timeout_ms, options.abort.clone(), &mut tasks).await;
        abort_stream_task(tasks.stdout_task.take());
        abort_stream_task(tasks.stderr_task.take());
        self.untrack_child(pid);

        exec_outcome_to_result(wait, tasks.stdout, tasks.stderr, &options)
    }

    fn track_child(&self, pid: Option<u32>) {
        if let Some(pid) = pid {
            self.active_child_pids
                .lock()
                .expect("active child pids lock")
                .insert(pid);
        }
    }

    fn untrack_child(&self, pid: Option<u32>) {
        if let Some(pid) = pid {
            self.active_child_pids
                .lock()
                .expect("active child pids lock")
                .remove(&pid);
        }
    }

    fn spawn_shell(
        &self,
        shell: &ShellConfig,
        command: &JsString,
        cwd: &JsString,
        options: &ShellExecOptions,
    ) -> Result<tokio::process::Child, ExecutionError> {
        let mut process = Command::new(&shell.shell);
        process.args(&shell.args).arg(encode_node_utf8(command));
        process.current_dir(encode_node_utf8(cwd));
        process.env_clear();
        process.envs(build_env(&self.shell_env, options.env.as_ref(), options.inherit_env));
        process.stdin(std::process::Stdio::null());
        process.stdout(std::process::Stdio::piped());
        process.stderr(std::process::Stdio::piped());
        #[cfg(unix)]
        process.process_group(0);

        process.spawn().map_err(|error| {
            ExecutionError::new(ExecutionErrorCode::SpawnError, error.to_string(), Some(Box::new(error)))
        })
    }
}

fn resolve_exec_cwd(env_cwd: &JsString, requested_cwd: Option<&JsString>) -> JsString {
    match requested_cwd {
        Some(path) => resolve_path(env_cwd, path),
        None => env_cwd.to_owned(),
    }
}

async fn ensure_working_directory(cwd: &JsString) -> Result<(), ExecutionError> {
    if path_exists(cwd).await {
        return Ok(());
    }
    Err(ExecutionError::new(
        ExecutionErrorCode::SpawnError,
        js_format!(
            "Working directory does not exist: {}\nCannot execute bash commands.",
            cwd
        ),
        None,
    ))
}

struct StreamTasks {
    stdout_task: Option<tokio::task::JoinHandle<()>>,
    stderr_task: Option<tokio::task::JoinHandle<()>>,
    receiver: UnboundedReceiver<StreamEvent>,
    stdout: Arc<Mutex<String>>,
    stderr: Arc<Mutex<String>>,
}

fn start_stream_tasks(
    child: &mut tokio::process::Child,
    on_stdout: Option<Arc<ShellOutputHandler>>,
    on_stderr: Option<Arc<ShellOutputHandler>>,
) -> StreamTasks {
    let stdout = Arc::new(Mutex::new(String::new()));
    let stderr = Arc::new(Mutex::new(String::new()));
    let (sender, receiver) = mpsc::unbounded_channel();
    let stdout_task = child.stdout.take().map(|reader| {
        tokio::spawn(read_stream(
            reader,
            StreamKind::Stdout,
            Arc::clone(&stdout),
            on_stdout,
            sender.clone(),
        ))
    });
    let stderr_task = child.stderr.take().map(|reader| {
        tokio::spawn(read_stream(
            reader,
            StreamKind::Stderr,
            Arc::clone(&stderr),
            on_stderr,
            sender,
        ))
    });
    StreamTasks {
        stdout_task,
        stderr_task,
        receiver,
        stdout,
        stderr,
    }
}

struct ExitWait {
    status: Result<std::process::ExitStatus, std::io::Error>,
    timed_out: bool,
    aborted: bool,
    callback_error: Option<JsString>,
}

#[derive(Default)]
struct WaitFlags {
    timed_out: bool,
    aborted: bool,
    callback_error: Option<JsString>,
}

async fn wait_for_exit(
    child: &mut tokio::process::Child,
    pid: Option<u32>,
    timeout_ms: Option<f64>,
    abort: Option<CancellationToken>,
    tasks: &mut StreamTasks,
) -> ExitWait {
    let deadline = timeout_ms.map(|millis| tokio::time::Instant::now() + Duration::from_secs_f64(millis / 1000.0));
    let mut flags = WaitFlags::default();
    let mut done = [false, false];
    let status = wait_for_status(child, pid, deadline, abort.as_ref(), tasks, &mut done, &mut flags).await;
    drain_streams(&mut tasks.receiver, &mut done, &mut flags.callback_error).await;
    ExitWait {
        status,
        timed_out: flags.timed_out,
        aborted: flags.aborted,
        callback_error: flags.callback_error,
    }
}

async fn wait_for_status(
    child: &mut tokio::process::Child,
    pid: Option<u32>,
    deadline: Option<tokio::time::Instant>,
    abort: Option<&CancellationToken>,
    tasks: &mut StreamTasks,
    done: &mut [bool; 2],
    flags: &mut WaitFlags,
) -> Result<std::process::ExitStatus, std::io::Error> {
    let mut killed_for_callback = false;
    loop {
        tokio::select! {
            status = child.wait() => return status,
            _ = sleep_until_deadline(deadline), if !flags.timed_out => {
                flags.timed_out = true;
                kill_if_running(pid);
            }
            _ = cancel_on_abort(abort), if !flags.aborted => {
                flags.aborted = true;
                kill_if_running(pid);
            }
            event = tasks.receiver.recv() => {
                apply_stream_event(event, done, &mut flags.callback_error);
                if flags.callback_error.is_some() && !killed_for_callback {
                    killed_for_callback = true;
                    kill_if_running(pid);
                }
            }
        }
    }
}

async fn sleep_until_deadline(deadline: Option<tokio::time::Instant>) {
    match deadline {
        Some(deadline) => tokio::time::sleep_until(deadline).await,
        None => std::future::pending::<()>().await,
    }
}

async fn cancel_on_abort(abort: Option<&CancellationToken>) {
    match abort {
        Some(token) => token.cancelled().await,
        None => std::future::pending::<()>().await,
    }
}

fn kill_if_running(pid: Option<u32>) {
    if let Some(pid) = pid {
        kill_process_tree(pid);
    }
}

async fn drain_streams(
    receiver: &mut UnboundedReceiver<StreamEvent>,
    done: &mut [bool; 2],
    callback_error: &mut Option<JsString>,
) {
    let grace = tokio::time::sleep(EXIT_STDIO_GRACE);
    tokio::pin!(grace);
    while !(done[0] && done[1]) {
        tokio::select! {
            _ = &mut grace => break,
            event = receiver.recv() => {
                if apply_stream_event(event, done, callback_error) {
                    break;
                }
            }
        }
    }
}

fn abort_stream_task(task: Option<tokio::task::JoinHandle<()>>) {
    if let Some(task) = task {
        task.abort();
    }
}

fn exec_outcome_to_result(
    wait: ExitWait,
    stdout: Arc<Mutex<String>>,
    stderr: Arc<Mutex<String>>,
    options: &ShellExecOptions,
) -> Result<ShellExecResult, ExecutionError> {
    if let Some(message) = wait.callback_error {
        return Err(ExecutionError::new(ExecutionErrorCode::CallbackError, message, None));
    }
    if wait.timed_out {
        return Err(ExecutionError::new(
            ExecutionErrorCode::Timeout,
            format!("timeout:{}", js_number_to_string(options.timeout.unwrap_or_default())),
            None,
        ));
    }
    let cancelled = options.abort.as_ref().is_some_and(CancellationToken::is_cancelled);
    if wait.aborted || cancelled {
        return Err(ExecutionError::new(ExecutionErrorCode::Aborted, "aborted", None));
    }
    let status = wait.status.map_err(|error| {
        ExecutionError::new(ExecutionErrorCode::SpawnError, error.to_string(), Some(Box::new(error)))
    })?;
    Ok(ShellExecResult {
        stdout: stdout.lock().expect("stdout lock").clone(),
        stderr: stderr.lock().expect("stderr lock").clone(),
        exit_code: status.code().unwrap_or(0),
    })
}

#[derive(Debug, Clone, Copy)]
enum StreamKind {
    Stdout,
    Stderr,
}

struct StreamEvent {
    kind: StreamKind,
    result: Result<(), JsString>,
}

fn apply_stream_event(event: Option<StreamEvent>, done: &mut [bool; 2], callback_error: &mut Option<JsString>) -> bool {
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
    callback: Option<Arc<ShellOutputHandler>>,
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
                if !deliver_chunk(kind, &text, &shared, callback.as_ref(), &sender) {
                    return;
                }
            }
            Err(error) => {
                let _ = sender.send(StreamEvent {
                    kind,
                    result: Err(error.to_string().into()),
                });
                return;
            }
        }
    }
    let tail = decoder.finish();
    if !deliver_chunk(kind, &tail, &shared, callback.as_ref(), &sender) {
        return;
    }
    let _ = sender.send(StreamEvent { kind, result: Ok(()) });
}

fn deliver_chunk(
    kind: StreamKind,
    text: &str,
    shared: &Arc<Mutex<String>>,
    callback: Option<&Arc<ShellOutputHandler>>,
    sender: &UnboundedSender<StreamEvent>,
) -> bool {
    if text.is_empty() {
        return true;
    }
    shared.lock().expect("stream text lock").push_str(text);
    if let Some(callback) = callback
        && let Err(message) = callback(text)
    {
        let _ = sender.send(StreamEvent {
            kind,
            result: Err(message),
        });
        return false;
    }
    true
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
                js_number_to_string(MAX_TIMEOUT_SECONDS)
            ),
            None,
        ));
    }
    Ok(Some(timeout_ms))
}

fn build_env(
    shell_env: &HashMap<String, String>,
    extra_env: Option<&HashMap<String, String>>,
    inherit_env: Option<bool>,
) -> HashMap<String, String> {
    if inherit_env == Some(false) {
        return extra_env.cloned().unwrap_or_default();
    }
    let mut environment = pi_js_compat::process_env::vars().into_iter().collect::<HashMap<_, _>>();
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
        if path_exists(&custom_shell_path.into()).await {
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
    if path_exists(&"/bin/bash".into()).await {
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
    if first_match.is_empty() || !path_exists(&first_match.into()).await {
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

async fn path_exists(path: &JsString) -> bool {
    tokio::fs::metadata(encode_node_utf8(path)).await.is_ok()
}

fn kill_process_tree(pid: u32) {
    let Some(pid) = rustix::process::Pid::from_raw(pid as i32) else {
        return;
    };
    let _ = rustix::process::kill_process_group(pid, rustix::process::Signal::KILL);
}

async fn create_parent_dir(resolved: &JsString) -> Result<(), FileError> {
    let parent = resolved
        .units()
        .iter()
        .rposition(|unit| *unit == 0x2f)
        .map(|index| resolved.slice(0, index.max(1)))
        .unwrap_or_else(|| resolved.clone());
    tokio::fs::create_dir_all(encode_node_utf8(&parent))
        .await
        .map_err(|error| file_error(&error, "mkdir", &parent))
}

async fn remove_path(resolved: &JsString, options: RemoveOptions) -> Result<(), FileError> {
    let metadata = match tokio::fs::symlink_metadata(encode_node_utf8(resolved)).await {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound && options.force.unwrap_or(false) => return Ok(()),
        Err(error) => return Err(file_error(&error, "lstat", resolved)),
    };
    if metadata.is_dir() && !metadata.file_type().is_symlink() {
        if !options.recursive.unwrap_or(false) {
            return Err(FileError::new(
                FileErrorCode::Unknown,
                js_format!("Path is a directory: rm returned EISDIR (is a directory) {}", resolved),
                Some(resolved.to_owned()),
                None,
            ));
        }
        tokio::fs::remove_dir_all(encode_node_utf8(resolved))
            .await
            .map_err(|error| file_error(&error, "rmdir", resolved))
    } else {
        tokio::fs::remove_file(encode_node_utf8(resolved))
            .await
            .map_err(|error| file_error(&error, "unlink", resolved))
    }
}

async fn create_temp_dir(prefix: &JsString) -> Result<JsString, FileError> {
    let base = JsString::from(std::env::temp_dir().to_string_lossy().into_owned());
    for _ in 0..100 {
        let candidate = join_paths(&[base.clone(), prefix.clone() + random_name_suffix()]);
        match tokio::fs::create_dir(encode_node_utf8(&candidate)).await {
            Ok(()) => return Ok(candidate),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(file_error(&error, "mkdir", &candidate)),
        }
    }
    Err(FileError::new(
        FileErrorCode::Unknown,
        "Could not create a unique temporary directory",
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

fn abort_result(abort: Option<&CancellationToken>, path: &JsString) -> Option<FileError> {
    abort
        .filter(|token| token.is_cancelled())
        .map(|_| FileError::new(FileErrorCode::Aborted, "aborted", Some(path.to_owned()), None))
}

fn file_info_from_metadata(path: &JsString, metadata: &std::fs::Metadata) -> Result<FileInfo, FileError> {
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
        name: path.split('/').last().unwrap_or_default(),
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

fn file_error(error: &io::Error, syscall: &str, path: &JsString) -> FileError {
    let (code, description, error_code) = match error_parts(error) {
        Some(parts) => parts,
        None => return unknown_file_error(error, syscall, path),
    };
    FileError::new(
        error_code,
        js_format!("{}: {}, {} '{}'", code, description, syscall, path),
        Some(path.to_owned()),
        Some(Box::new(io::Error::new(error.kind(), error.to_string()))),
    )
}

fn rename_error(error: &io::Error, source: &JsString, destination: &JsString) -> FileError {
    let (code, description, error_code) = match error_parts(error) {
        Some(parts) => parts,
        None => return unknown_file_error(error, "rename", source),
    };
    FileError::new(
        error_code,
        js_format!("{}: {}, rename '{}' -> '{}'", code, description, source, destination),
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

fn unknown_file_error(error: &io::Error, syscall: &str, path: &JsString) -> FileError {
    FileError::new(
        FileErrorCode::Unknown,
        js_format!("UNKNOWN: {}, {} '{}'", error.to_string(), syscall, path),
        Some(path.to_owned()),
        Some(Box::new(io::Error::new(error.kind(), error.to_string()))),
    )
}

async fn read_sorted_directory(path: &JsString) -> Result<Vec<tokio::fs::DirEntry>, FileError> {
    let mut directory = tokio::fs::read_dir(encode_node_utf8(path))
        .await
        .map_err(|error| file_error(&error, "scandir", path))?;
    let mut entries = Vec::new();
    while let Some(entry) = directory
        .next_entry()
        .await
        .map_err(|error| file_error(&error, "scandir", path))?
    {
        entries.push(entry);
    }
    // Node's libuv scandir orders native names before decoding and lstat.
    entries.sort_by_key(tokio::fs::DirEntry::file_name);
    Ok(entries)
}

fn resolve_path(cwd: &JsString, path: &JsString) -> JsString {
    let normalized = expand_path(path);
    let absolute = if normalized.starts_with('/') {
        normalized
    } else {
        let base = if cwd.starts_with('/') {
            cwd.clone()
        } else {
            std::env::current_dir()
                .map(|current| JsString::from(current.to_string_lossy().into_owned()))
                .unwrap_or_else(|_| "/".into())
        };
        base + "/" + normalized
    };
    node_normalize(&absolute)
}

fn expand_path(path: &JsString) -> JsString {
    if path == "~" {
        return home_dir();
    }
    if let Some(rest) = path.strip_prefix("~/") {
        return home_dir() + "/" + rest;
    }
    if path.starts_with("file://")
        && let Some(decoded) = file_url_to_path(path)
    {
        return decoded;
    }
    path.clone()
}

fn home_dir() -> JsString {
    pi_js_compat::process_env::get("HOME")
        .unwrap_or_else(|| "/".to_owned())
        .into()
}

fn file_url_to_path(url: &JsString) -> Option<JsString> {
    let rest = url.strip_prefix("file://")?;
    let path = if let Some(path) = rest.strip_prefix("localhost/") {
        JsString::from("/") + path
    } else if rest.starts_with('/') {
        rest
    } else {
        return None;
    };
    Some(percent_decode(&path))
}

fn percent_decode(value: &JsString) -> JsString {
    let units = value.units();
    let mut output = JsString::default();
    let mut index = 0;
    while index < units.len() {
        let mut bytes = Vec::new();
        while index + 2 < units.len() && units[index] == 0x25 {
            let Some(high) = hex_value(units[index + 1]) else {
                break;
            };
            let Some(low) = hex_value(units[index + 2]) else {
                break;
            };
            bytes.push(high << 4 | low);
            index += 3;
        }
        if bytes.is_empty() {
            output.push_unit(units[index]);
            index += 1;
        } else {
            output.push_str(String::from_utf8_lossy(&bytes).into_owned());
        }
    }
    output
}

fn hex_value(unit: u16) -> Option<u8> {
    match unit {
        0x30..=0x39 => Some((unit - 0x30) as u8),
        0x61..=0x66 => Some((unit - 0x61 + 10) as u8),
        0x41..=0x46 => Some((unit - 0x41 + 10) as u8),
        _ => None,
    }
}

fn join_paths(parts: &[JsString]) -> JsString {
    let joined = JsString::join(parts.iter().filter(|part| !part.is_empty()), "/");
    if joined.is_empty() {
        ".".into()
    } else {
        node_normalize(&joined)
    }
}

fn node_normalize(path: &JsString) -> JsString {
    let absolute = path.starts_with('/');
    let mut components: Vec<JsString> = Vec::new();
    for component in path.split('/') {
        if component.is_empty() || component == "." {
            continue;
        }
        if component == ".." {
            if components.last().is_some_and(|last| last != "..") {
                components.pop();
            } else if !absolute {
                components.push("..".into());
            }
        } else {
            components.push(component);
        }
    }
    let joined = JsString::join(components, "/");
    if absolute {
        JsString::from("/") + joined
    } else if joined.is_empty() {
        ".".into()
    } else {
        joined
    }
}
