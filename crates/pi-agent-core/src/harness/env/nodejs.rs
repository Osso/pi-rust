//! Port of `packages/agent/src/harness/env/nodejs.ts`.
//!
//! Windows-specific shell discovery and WSL stdin transport branches are excluded; the port
//! targets the Unix runtime the harness executes on.

mod read_lines;
use crate::harness::types::{NativeNodeError, NodeRejection, OperationFailure};

use indexmap::IndexMap;
use std::io;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use pi_ai::request::BoxFuture;
use pi_js_compat::js_format;
use pi_js_compat::js_json::{JsString, JsValue, encode_node_utf8};
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
static TEMP_NAME_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Options accepted by [`NodeExecutionEnv::new`].
#[derive(Debug, Clone, Default)]
pub struct NodeExecutionEnvOptions {
    pub cwd: JsString,
    pub shell_path: Option<JsString>,
    pub shell_env: IndexMap<String, Option<JsString>>,
}

/// Filesystem and process execution environment backed by the host OS.
#[derive(Debug)]
pub struct NodeExecutionEnv {
    cwd: JsString,
    shell_path: Option<JsString>,
    shell_env: IndexMap<String, Option<JsString>>,
}

impl NodeExecutionEnv {
    pub fn new(options: NodeExecutionEnvOptions) -> Self {
        Self {
            cwd: options.cwd,
            shell_path: options.shell_path,
            shell_env: options.shell_env,
        }
    }

    /// Frozen local environment owns no cleanup resources.
    pub async fn cleanup(&self) {}
}

impl FileSystem for NodeExecutionEnv {
    fn cwd(&self) -> &JsString {
        &self.cwd
    }

    fn absolute_path<'a>(
        &'a self,
        path: &'a JsString,
        _abort: Option<&'a CancellationToken>,
    ) -> BoxFuture<'a, Result<JsString, OperationFailure<FileError>>> {
        Box::pin(async move { resolve_path(&self.cwd, path).map_err(OperationFailure::Rejected) })
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
    ) -> BoxFuture<'a, Result<String, OperationFailure<FileError>>> {
        Box::pin(async move {
            let resolved = resolve_path(&self.cwd, path).map_err(OperationFailure::Rejected)?;
            async move {
                if let Some(error) = abort_result(abort, &resolved) {
                    return Err(error);
                }
                let bytes = tokio::fs::read(&encode_node_utf8(&resolved))
                    .await
                    .map_err(|error| read_file_error(&error, &resolved))?;
                io_abort_result(abort, &resolved)?;
                Ok(String::from_utf8_lossy(&bytes).into_owned())
            }
            .await
            .map_err(OperationFailure::Returned)
        })
    }

    fn read_text_lines<'a>(
        &'a self,
        path: &'a JsString,
        options: ReadTextLinesOptions,
    ) -> BoxFuture<'a, Result<Vec<String>, OperationFailure<FileError>>> {
        Box::pin(async move {
            let resolved = resolve_path(&self.cwd, path).map_err(OperationFailure::Rejected)?;
            async move {
                if let Some(error) = abort_result(options.abort.as_ref(), &resolved) {
                    return Err(error);
                }
                if options.max_lines.is_some_and(|limit| limit <= 0.0) {
                    return Ok(Vec::new());
                }
                read_lines::read_text_lines(&resolved, options).await
            }
            .await
            .map_err(OperationFailure::Returned)
        })
    }

    fn read_binary_file<'a>(
        &'a self,
        path: &'a JsString,
        abort: Option<&'a CancellationToken>,
    ) -> BoxFuture<'a, Result<Vec<u8>, OperationFailure<FileError>>> {
        Box::pin(async move {
            let resolved = resolve_path(&self.cwd, path).map_err(OperationFailure::Rejected)?;
            async move {
                if let Some(error) = abort_result(abort, &resolved) {
                    return Err(error);
                }
                let bytes = tokio::fs::read(&encode_node_utf8(&resolved))
                    .await
                    .map_err(|error| read_file_error(&error, &resolved))?;
                io_abort_result(abort, &resolved)?;
                Ok(bytes)
            }
            .await
            .map_err(OperationFailure::Returned)
        })
    }

    fn write_file<'a>(
        &'a self,
        path: &'a JsString,
        content: FileContent<'a>,
        abort: Option<&'a CancellationToken>,
    ) -> BoxFuture<'a, Result<(), OperationFailure<FileError>>> {
        Box::pin(async move {
            let resolved = resolve_path(&self.cwd, path).map_err(OperationFailure::Rejected)?;
            async move {
                if let Some(error) = abort_result(abort, &resolved) {
                    return Err(error);
                }
                create_parent_dir(&resolved).await?;
                if let Some(error) = abort_result(abort, &resolved) {
                    return Err(error);
                }
                tokio::fs::write(&encode_node_utf8(&resolved), content.as_bytes())
                    .await
                    .map_err(|error| file_error(&error, "open", &resolved))?;
                io_abort_result(abort, &resolved)
            }
            .await
            .map_err(OperationFailure::Returned)
        })
    }

    fn append_file<'a>(
        &'a self,
        path: &'a JsString,
        content: FileContent<'a>,
        _abort: Option<&'a CancellationToken>,
    ) -> BoxFuture<'a, Result<(), OperationFailure<FileError>>> {
        Box::pin(async move {
            let resolved = resolve_path(&self.cwd, path).map_err(OperationFailure::Rejected)?;
            async move {
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
            }
            .await
            .map_err(OperationFailure::Returned)
        })
    }

    fn file_info<'a>(&'a self, path: &'a JsString) -> BoxFuture<'a, Result<FileInfo, OperationFailure<FileError>>> {
        Box::pin(async move {
            let resolved = resolve_path(&self.cwd, path).map_err(OperationFailure::Rejected)?;
            async move {
                let metadata = tokio::fs::symlink_metadata(&encode_node_utf8(&resolved))
                    .await
                    .map_err(|error| file_error(&error, "lstat", &resolved))?;
                file_info_from_metadata(&resolved, &metadata)
            }
            .await
            .map_err(OperationFailure::Returned)
        })
    }

    fn list_dir<'a>(
        &'a self,
        path: &'a JsString,
        abort: Option<&'a CancellationToken>,
    ) -> BoxFuture<'a, Result<Vec<FileInfo>, OperationFailure<FileError>>> {
        Box::pin(async move {
            let resolved = resolve_path(&self.cwd, path).map_err(OperationFailure::Rejected)?;
            async move {
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
                        pi_js_compat::node_fs::decode_node_path(entry.file_name().as_os_str()),
                    ]);
                    let metadata = tokio::fs::symlink_metadata(&encode_node_utf8(&entry_path))
                        .await
                        .map_err(|error| file_error(&error, "lstat", &entry_path))?;
                    if let Ok(info) = file_info_from_metadata(&entry_path, &metadata) {
                        infos.push(info);
                    }
                }
                Ok(infos)
            }
            .await
            .map_err(OperationFailure::Returned)
        })
    }

    fn canonical_path<'a>(
        &'a self,
        path: &'a JsString,
    ) -> BoxFuture<'a, Result<JsString, OperationFailure<FileError>>> {
        Box::pin(async move {
            let resolved = resolve_path(&self.cwd, path).map_err(OperationFailure::Rejected)?;
            async move {
                tokio::fs::canonicalize(&encode_node_utf8(&resolved))
                    .await
                    .map(|path| pi_js_compat::node_fs::decode_node_path(path.as_os_str()))
                    .map_err(|error| file_error(&error, "realpath", &resolved))
            }
            .await
            .map_err(OperationFailure::Returned)
        })
    }

    fn exists<'a>(
        &'a self,
        path: &'a JsString,
        _abort: Option<&'a CancellationToken>,
    ) -> BoxFuture<'a, Result<bool, OperationFailure<FileError>>> {
        Box::pin(async move {
            match self.file_info(path).await {
                Ok(_) => Ok(true),
                Err(OperationFailure::Returned(error)) if error.code == FileErrorCode::NotFound => Ok(false),
                Err(error) => Err(error),
            }
        })
    }

    fn create_dir<'a>(
        &'a self,
        path: &'a JsString,
        options: CreateDirOptions,
    ) -> BoxFuture<'a, Result<(), OperationFailure<FileError>>> {
        Box::pin(async move {
            let resolved = resolve_path(&self.cwd, path).map_err(OperationFailure::Rejected)?;
            async move {
                let result = if options.recursive.unwrap_or(true) {
                    tokio::fs::create_dir_all(&encode_node_utf8(&resolved)).await
                } else {
                    tokio::fs::create_dir(&encode_node_utf8(&resolved)).await
                };
                result.map_err(|error| file_error(&error, "mkdir", &resolved))
            }
            .await
            .map_err(OperationFailure::Returned)
        })
    }

    fn remove<'a>(
        &'a self,
        path: &'a JsString,
        options: RemoveOptions,
    ) -> BoxFuture<'a, Result<(), OperationFailure<FileError>>> {
        Box::pin(async move {
            let resolved = resolve_path(&self.cwd, path).map_err(OperationFailure::Rejected)?;
            async move { remove_path(&resolved, options).await }
                .await
                .map_err(OperationFailure::Returned)
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
                pi_ai::utils::random_uuid::random_uuid(),
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
        Box::pin(async {})
    }
}

impl Shell for NodeExecutionEnv {
    fn exec<'a>(
        &'a self,
        command: &'a JsString,
        options: ShellExecOptions,
    ) -> BoxFuture<'a, Result<ShellExecResult, OperationFailure<ExecutionError>>> {
        Box::pin(async move { self.exec_command(command, options).await })
    }

    fn cleanup(&self) -> BoxFuture<'_, ()> {
        Box::pin(async {})
    }
}

impl NodeExecutionEnv {
    async fn exec_command(
        &self,
        command: &JsString,
        options: ShellExecOptions,
    ) -> Result<ShellExecResult, OperationFailure<ExecutionError>> {
        if options.abort.as_ref().is_some_and(CancellationToken::is_cancelled) {
            return Err(ExecutionError::new(ExecutionErrorCode::Aborted, "aborted", None).into());
        }
        let timeout_ms = resolve_timeout_ms(options.timeout)?;
        let cwd = resolve_exec_cwd(&self.cwd, options.cwd.as_ref()).map_err(OperationFailure::Rejected)?;
        let shell = get_shell_config(self.shell_path.as_ref()).await?;

        let mut child = self.spawn_shell(&shell, command, &cwd, &options)?;
        let pid = child.id();

        let mut tasks = start_stream_tasks(&mut child, options.on_stdout.clone(), options.on_stderr.clone());
        let wait = wait_for_exit(&mut child, pid, timeout_ms, options.abort.clone(), &mut tasks).await;
        abort_stream_task(tasks.stdout_task.take());
        abort_stream_task(tasks.stderr_task.take());

        exec_outcome_to_result(wait, tasks.stdout, tasks.stderr, &options).map_err(OperationFailure::Returned)
    }

    fn spawn_shell(
        &self,
        shell: &ShellConfig,
        command: &JsString,
        cwd: &JsString,
        options: &ShellExecOptions,
    ) -> Result<tokio::process::Child, ExecutionError> {
        let environment = build_env(&self.shell_env, options.env.as_ref());
        validate_spawn_arguments(command, cwd, &environment)?;
        let mut process = Command::new(encode_node_utf8(&shell.shell));
        process.args(&shell.args).arg(encode_node_utf8(command));
        process.current_dir(encode_node_utf8(cwd));
        process.env_clear();
        process.envs(
            environment
                .into_iter()
                .filter_map(|(key, value)| value.map(|value| (key, encode_node_utf8(&value)))),
        );
        process.stdin(std::process::Stdio::null());
        process.stdout(std::process::Stdio::piped());
        process.stderr(std::process::Stdio::piped());
        #[cfg(unix)]
        process.process_group(0);

        process.spawn().map_err(|error| {
            let cause = NativeNodeError::from(pi_js_compat::node_fs::node_spawn_error_record(&error, &shell.shell));
            ExecutionError::new(
                ExecutionErrorCode::SpawnError,
                cause.message.clone(),
                Some(Box::new(cause)),
            )
        })
    }
}

fn resolve_exec_cwd(env_cwd: &JsString, requested_cwd: Option<&JsString>) -> Result<JsString, NodeRejection> {
    match requested_cwd {
        Some(path) if !path.is_empty() => resolve_path(env_cwd, path),
        _ => Ok(env_cwd.to_owned()),
    }
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
    let deadline = timeout_ms.map(|millis| {
        tokio::time::Instant::now()
            + Duration::from_millis(pi_js_compat::node_timers::normalize_timeout_delay(millis) as u64)
    });
    let mut flags = WaitFlags::default();
    let mut done = [false, false];
    let status = wait_for_status(child, pid, deadline, abort.as_ref(), tasks, &mut done, &mut flags).await;
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
    let mut status = None;
    loop {
        if done[0]
            && done[1]
            && let Some(status) = status.take()
        {
            return status;
        }
        tokio::select! {
            result = child.wait(), if status.is_none() => status = Some(result),
            _ = sleep_until_deadline(deadline), if !flags.timed_out => {
                flags.timed_out = true;
                kill_if_running(pid);
            }
            _ = cancel_on_abort(abort), if !flags.aborted => {
                flags.aborted = true;
                kill_if_running(pid);
            }
            event = tasks.receiver.recv(), if !(done[0] && done[1]) => {
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
        let cause = NativeNodeError::new("Error", message.clone());
        return Err(ExecutionError::new(
            ExecutionErrorCode::CallbackError,
            message,
            Some(Box::new(cause)),
        ));
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
    complete: bool,
}

fn apply_stream_event(event: Option<StreamEvent>, done: &mut [bool; 2], callback_error: &mut Option<JsString>) -> bool {
    let Some(event) = event else {
        return true;
    };
    done[match event.kind {
        StreamKind::Stdout => 0,
        StreamKind::Stderr => 1,
    }] |= event.complete;
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
                deliver_chunk(kind, &text, &shared, callback.as_ref(), &sender);
            }
            Err(error) => {
                let _ = sender.send(StreamEvent {
                    kind,
                    result: Err(error.to_string().into()),
                    complete: true,
                });
                return;
            }
        }
    }
    let tail = decoder.finish();
    deliver_chunk(kind, &tail, &shared, callback.as_ref(), &sender);
    let _ = sender.send(StreamEvent {
        kind,
        result: Ok(()),
        complete: true,
    });
}

fn deliver_chunk(
    kind: StreamKind,
    text: &str,
    shared: &Arc<Mutex<String>>,
    callback: Option<&Arc<ShellOutputHandler>>,
    sender: &UnboundedSender<StreamEvent>,
) {
    if text.is_empty() {
        return;
    }
    shared.lock().expect("stream text lock").push_str(text);
    if let Some(callback) = callback
        && let Err(message) = callback(text)
    {
        let _ = sender.send(StreamEvent {
            kind,
            result: Err(message),
            complete: false,
        });
    }
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
    shell_env: &IndexMap<String, Option<JsString>>,
    extra_env: Option<&IndexMap<String, JsString>>,
) -> IndexMap<String, Option<JsString>> {
    let mut environment: IndexMap<_, _> = pi_js_compat::process_env::vars()
        .into_iter()
        .map(|(key, value)| (key.into(), Some(value.into())))
        .collect();
    environment.extend(shell_env.iter().map(|(key, value)| (key.clone(), value.clone())));
    if let Some(extra_env) = extra_env {
        environment.extend(extra_env.iter().map(|(key, value)| (key.clone(), Some(value.clone()))));
    }
    let names = environment
        .keys()
        .map(|key| (key.clone(), serde_json::Value::Null))
        .collect();
    pi_js_compat::js_json::object_entries(&names)
        .into_iter()
        .map(|(key, _)| {
            (
                key.clone(),
                environment.get(key).expect("enumerated environment key").clone(),
            )
        })
        .collect()
}

fn validate_spawn_arguments(
    command: &JsString,
    cwd: &JsString,
    env: &IndexMap<String, Option<JsString>>,
) -> Result<(), ExecutionError> {
    validate_spawn_string("args[1]", "must be a string without null bytes", command)?;
    validate_spawn_string(
        "options.cwd",
        "must be a string, Uint8Array, or URL without null bytes",
        cwd,
    )?;
    for (key, value) in env {
        if let Some(value) = value {
            let name = format!("options.env['{key}']");
            validate_spawn_string(&name, "must be a string without null bytes", &key.as_str().into())?;
            validate_spawn_string(&name, "must be a string without null bytes", value)?;
        }
    }
    Ok(())
}

fn argument_error(name: &str, reason: &str, value: &JsString) -> NativeNodeError {
    NativeNodeError::new(
        "TypeError",
        pi_js_compat::node_fs::invalid_argument_value_message(name, reason, value),
    )
    .with_property("code", JsValue::String("ERR_INVALID_ARG_VALUE".into()))
}

fn validate_spawn_string(name: &str, reason: &str, value: &JsString) -> Result<(), ExecutionError> {
    if value.contains('\0') {
        let cause = argument_error(name, reason, value);
        return Err(ExecutionError::new(
            ExecutionErrorCode::SpawnError,
            cause.message.clone(),
            Some(Box::new(cause)),
        ));
    }
    Ok(())
}

struct ShellConfig {
    shell: JsString,
    args: Vec<String>,
}

async fn get_shell_config(custom_shell_path: Option<&JsString>) -> Result<ShellConfig, ExecutionError> {
    if let Some(custom_shell_path) = custom_shell_path.filter(|path| !path.is_empty()) {
        if path_exists(custom_shell_path).await {
            return Ok(ShellConfig {
                shell: custom_shell_path.to_owned(),
                args: vec!["-c".to_owned()],
            });
        }
        return Err(ExecutionError::new(
            ExecutionErrorCode::ShellUnavailable,
            js_format!("Custom shell path not found: {}", custom_shell_path),
            None,
        ));
    }
    if path_exists(&"/bin/bash".into()).await {
        return Ok(ShellConfig {
            shell: "/bin/bash".into(),
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
        shell: "sh".into(),
        args: vec!["-c".to_owned()],
    })
}

async fn find_bash_on_path() -> Option<JsString> {
    let (stdout, status) = run_command("which", &["bash"], 5000).await?;
    if status != 0 || stdout.is_empty() {
        return None;
    }
    let trimmed = JsString::from(stdout).trim();
    let first_match = trimmed.split('\n').next()?;
    let first_match = first_match.strip_suffix("\r").unwrap_or(first_match);
    if first_match.is_empty() || !path_exists(&first_match).await {
        return None;
    }
    Some(first_match)
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
    if rustix::process::kill_process_group(pid, rustix::process::Signal::KILL).is_err() {
        let _ = rustix::process::kill_process(pid, rustix::process::Signal::KILL);
    }
}

async fn create_parent_dir(resolved: &JsString) -> Result<(), FileError> {
    let parent = resolve_absolute_path(resolved, &"..".into());
    tokio::fs::create_dir_all(encode_node_utf8(&parent))
        .await
        .map_err(|error| {
            let mut error = file_error(&error, "mkdir", &parent);
            error.path = Some(resolved.clone());
            error
        })
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
    let base = pi_js_compat::node_fs::decode_node_path(std::env::temp_dir().as_os_str());
    let prefix = join_paths(&[base, prefix.clone()]);
    if prefix.contains('\0') {
        let cause = argument_error(
            "prefix",
            "must be a string, Uint8Array, or URL without null bytes",
            &prefix,
        );
        return Err(FileError::new(
            FileErrorCode::Unknown,
            cause.message.clone(),
            None,
            Some(Box::new(cause)),
        ));
    }
    for _ in 0..100 {
        let candidate = prefix.clone() + random_name_suffix();
        match tokio::fs::create_dir(encode_node_utf8(&candidate)).await {
            Ok(()) => return Ok(JsString::from(encode_node_utf8(&candidate))),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                let mut error = file_error(&error, "mkdtemp", &candidate);
                error.path = None;
                return Err(error);
            }
        }
    }
    Err(FileError::new(
        FileErrorCode::Unknown,
        "Could not create a unique temporary directory",
        None,
        None,
    ))
}

fn random_name_suffix() -> String {
    let counter = TEMP_NAME_COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    format!("{:06x}", (nanos as u64 ^ counter) & 0xff_ffff)
}

fn io_abort_result(abort: Option<&CancellationToken>, path: &JsString) -> Result<(), FileError> {
    if abort.is_some_and(CancellationToken::is_cancelled) {
        return Err(FileError::new(
            FileErrorCode::Aborted,
            "The operation was aborted",
            Some(path.clone()),
            None,
        ));
    }
    Ok(())
}

fn abort_result(abort: Option<&CancellationToken>, path: &JsString) -> Option<FileError> {
    abort
        .filter(|token| token.is_cancelled())
        .map(|_| FileError::new(FileErrorCode::Aborted, "aborted", Some(path.to_owned()), None))
}

fn file_basename(path: &JsString) -> JsString {
    let end = path
        .units()
        .iter()
        .rposition(|unit| *unit != u16::from(b'/'))
        .map_or(0, |index| index + 1);
    path.slice(0, end).split('/').last().unwrap_or_default()
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
        name: file_basename(path),
        path: path.to_owned(),
        kind,
        size: metadata.len() as f64,
        mtime_ms: {
            use std::os::unix::fs::MetadataExt;
            metadata.mtime() as f64 * 1000.0 + metadata.mtime_nsec() as f64 / 1_000_000.0
        },
    })
}

fn read_file_error(error: &io::Error, path: &JsString) -> FileError {
    let syscall = if error.kind() == io::ErrorKind::IsADirectory {
        "read"
    } else {
        "open"
    };
    file_error(error, syscall, path)
}

fn file_error(error: &io::Error, syscall: &str, path: &JsString) -> FileError {
    if let Some(invalid) = pi_js_compat::node_fs::node_path_argument_error(path) {
        let cause = NativeNodeError::from(invalid);
        return FileError::new(
            FileErrorCode::Unknown,
            cause.message.clone(),
            Some(path.clone()),
            Some(Box::new(cause)),
        );
    }
    let record = if syscall == "read" {
        pi_js_compat::node_fs::node_read_error_record(error, path, pi_js_compat::node_fs::NodeReadApi::PromisesReadFile)
    } else {
        pi_js_compat::node_fs::node_fs_error_record(error, syscall, path)
    };
    let code = match record.code {
        "ABORT_ERR" => FileErrorCode::Aborted,
        "ENOENT" => FileErrorCode::NotFound,
        "EACCES" | "EPERM" => FileErrorCode::PermissionDenied,
        "ENOTDIR" => FileErrorCode::NotDirectory,
        "EISDIR" => FileErrorCode::IsDirectory,
        "EINVAL" => FileErrorCode::Invalid,
        _ => FileErrorCode::Unknown,
    };
    let cause = NativeNodeError::from(record);
    FileError::new(code, cause.message.clone(), Some(path.clone()), Some(Box::new(cause)))
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

fn resolve_path(cwd: &JsString, path: &JsString) -> Result<JsString, NodeRejection> {
    if path.starts_with('/') {
        return Ok(path.clone());
    }
    let base = if cwd.starts_with('/') {
        cwd.clone()
    } else {
        let current = std::env::current_dir().map_err(|error| NodeRejection {
            error: pi_js_compat::node_fs::node_fs_error_record(&error, "uv_cwd", &JsString::default()).into(),
        })?;
        pi_js_compat::node_fs::decode_node_path(current.as_os_str()) + "/" + cwd
    };
    Ok(resolve_absolute_path(&base, path))
}

fn resolve_absolute_path(base: &JsString, path: &JsString) -> JsString {
    let normalized = node_normalize(&(base.clone() + "/" + path));
    if normalized != "/" {
        normalized.strip_suffix("/").unwrap_or(normalized)
    } else {
        normalized
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
    let trailing = path.ends_with('/');
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
    let joined = if trailing && !joined.is_empty() {
        joined + "/"
    } else {
        joined
    };
    if absolute {
        JsString::from("/") + joined
    } else if joined.is_empty() {
        ".".into()
    } else {
        joined
    }
}
