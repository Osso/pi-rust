use std::io;
use std::path::Path;
use std::process::Stdio;
use std::time::{Duration, Instant};

use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::{Child, Command};
use tokio::sync::mpsc;

pub use pi_js_compat::node_fs::{node_errno_name, node_error_code};

pub const EXIT_STDIO_GRACE_MS: u64 = 100;
const READ_BUFFER_BYTES: usize = 8192;

/// Byte sink for native child-process streaming without a duplicate capture buffer.
pub type ProcessDataCallback =
    std::sync::Arc<dyn Fn(&[u8]) -> Result<(), pi_js_compat::callback_error::CallbackError> + Send + Sync>;

/// Read Node-style data chunks, repairing UTF-8 separately for each read.
pub async fn read_utf8_chunks(mut reader: impl AsyncRead + Unpin) -> io::Result<String> {
    let mut bytes = [0; READ_BUFFER_BYTES];
    let mut output = String::new();
    loop {
        let count = reader.read(&mut bytes).await?;
        if count == 0 {
            return Ok(output);
        }
        output.push_str(&String::from_utf8_lossy(&bytes[..count]));
    }
}

/// Send default SIGTERM using the live child handle, never a PID cached across reaping.
pub fn send_child_sigterm(child: &tokio::process::Child) -> io::Result<()> {
    let Some(pid) = child.id().and_then(|pid| rustix::process::Pid::from_raw(pid as i32)) else {
        return Ok(());
    };
    match rustix::process::kill_process(pid, rustix::process::Signal::TERM) {
        Ok(()) | Err(rustix::io::Errno::SRCH) => Ok(()),
        Err(error) => Err(error.into()),
    }
}

/// Node reports signal names (`SIGTERM`), not numbers.
#[cfg(unix)]
pub fn signal_name(signal: i32) -> String {
    match signal {
        1 => "SIGHUP",
        2 => "SIGINT",
        3 => "SIGQUIT",
        4 => "SIGILL",
        6 => "SIGABRT",
        8 => "SIGFPE",
        9 => "SIGKILL",
        11 => "SIGSEGV",
        13 => "SIGPIPE",
        14 => "SIGALRM",
        15 => "SIGTERM",
        other => return format!("SIG{other}"),
    }
    .to_owned()
}

/// Name of the signal that terminated `status`, or `unknown` for platforms
/// without signal reporting.
#[cfg(unix)]
pub fn exit_signal_name(status: &std::process::ExitStatus) -> String {
    use std::os::unix::process::ExitStatusExt;

    status.signal().map_or_else(|| "unknown".to_owned(), signal_name)
}

/// Name of the signal that terminated `status`, or `unknown` for platforms
/// without signal reporting.
#[cfg(not(unix))]
pub fn exit_signal_name(_status: &std::process::ExitStatus) -> String {
    "unknown".to_owned()
}

/// Exit status plus the stdout/stderr drained while waiting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessOutput {
    pub status: Option<i32>,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

/// Spawn a child process with piped stdout/stderr and a null stdin.
pub fn spawn_process(command: &str, args: &[&str]) -> io::Result<Child> {
    Command::new(command)
        .args(args)
        .env_clear()
        .envs(pi_js_compat::process_env::vars())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
}

/// Spawn a child process synchronously and collect its output.
pub fn spawn_process_sync(command: &str, args: &[&str], cwd: Option<&Path>) -> io::Result<std::process::Output> {
    let mut process = std::process::Command::new(command);
    process
        .args(args)
        .env_clear()
        .envs(pi_js_compat::process_env::vars())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(cwd) = cwd {
        process.current_dir(cwd);
    }
    process.output()
}

/// Wait for a child process without hanging on inherited stdio handles.
///
/// After exit, the pipes are drained until they fall idle: the grace timer is
/// re-armed on every chunk, so an actively writing descendant keeps us reading
/// while a quiet inherited handle releases us after the grace elapses.
pub async fn wait_for_child_process(child: &mut Child) -> io::Result<ProcessOutput> {
    capture_child_process(child, None, None)
        .await
        .map(|capture| capture.output)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureStop {
    StdoutLimit,
    StderrLimit,
    Aborted,
}

#[derive(Debug)]
pub struct CapturedProcessOutput {
    pub output: ProcessOutput,
    pub stopped: Option<CaptureStop>,
}

/// Shared capture authority; a configured cap applies independently to both streams.
/// Stop/reap immediately on overflow or abort, retaining only bounded output bytes.
pub async fn capture_child_process(
    child: &mut Child,
    max_bytes: Option<usize>,
    signal: Option<&pi_js_compat::abort::AbortSignal>,
) -> io::Result<CapturedProcessOutput> {
    capture_with_sink(child, max_bytes, signal, OutputSink::Capture).await
}

/// Wait with the same post-exit idle grace while forwarding bytes without retaining
/// another output buffer. The callback is the required sink, not a test hook.
pub async fn wait_for_child_process_streaming(
    child: &mut Child,
    on_data: ProcessDataCallback,
) -> io::Result<ProcessOutput> {
    capture_with_sink(child, None, None, OutputSink::Stream(on_data))
        .await
        .map(|capture| capture.output)
}

enum OutputSink {
    Capture,
    Stream(ProcessDataCallback),
}

async fn capture_with_sink(
    child: &mut Child,
    max_bytes: Option<usize>,
    signal: Option<&pi_js_compat::abort::AbortSignal>,
    sink: OutputSink,
) -> io::Result<CapturedProcessOutput> {
    let (mut receiver, stdout_task, stderr_task) = start_output_readers(child);
    let mut state = WaitState::new(stdout_task.is_none(), stderr_task.is_none(), max_bytes, sink);
    collect_child_events(child, &mut receiver, &mut state, signal).await;
    let stopped = if (state.stopped.is_some() || state.error.is_some()) && !state.exited {
        stop_child_process(child, &mut state).await
    } else {
        Ok(())
    };
    abort_reader(stdout_task);
    abort_reader(stderr_task);
    while let Ok(event) = receiver.try_recv() {
        state.record_event(Some(event));
    }
    stopped?;
    if let Some(error) = state.error {
        return Err(error);
    }
    Ok(CapturedProcessOutput {
        output: state.output,
        stopped: state.stopped,
    })
}

async fn collect_child_events(
    child: &mut Child,
    receiver: &mut mpsc::Receiver<StreamEvent>,
    state: &mut WaitState,
    signal: Option<&pi_js_compat::abort::AbortSignal>,
) {
    let wait = child.wait();
    tokio::pin!(wait);
    loop {
        if state.is_complete() || state.stopped.is_some() || state.error.is_some() {
            break;
        }
        let idle = sleep_until_deadline(state.deadline);
        tokio::select! {
            status = &mut wait, if !state.exited => match status {
                Ok(status) => state.record_exit(status), Err(error) => state.error = Some(error),
            },
            event = receiver.recv(), if !state.pipes_ended() => state.record_event(event),
            () = wait_for_abort(signal), if signal.is_some() => state.stopped = Some(CaptureStop::Aborted),
            () = idle, if state.deadline.is_some() => break,
        }
    }
}

async fn stop_child_process(child: &mut Child, state: &mut WaitState) -> io::Result<()> {
    let status = match child.try_wait()? {
        Some(status) => status,
        None => {
            child.kill().await?;
            child.wait().await?
        }
    };
    state.record_exit(status);
    Ok(())
}

async fn wait_for_abort(signal: Option<&pi_js_compat::abort::AbortSignal>) {
    match signal {
        Some(signal) => {
            signal.wait().await;
        }
        None => std::future::pending().await,
    }
}

fn start_output_readers(
    child: &mut Child,
) -> (
    mpsc::Receiver<StreamEvent>,
    Option<tokio::task::JoinHandle<()>>,
    Option<tokio::task::JoinHandle<()>>,
) {
    let (sender, receiver) = mpsc::channel(64);
    let stdout_task = child
        .stdout
        .take()
        .map(|reader| tokio::spawn(read_stream(reader, Stream::Stdout, sender.clone())));
    let stderr_task = child
        .stderr
        .take()
        .map(|reader| tokio::spawn(read_stream(reader, Stream::Stderr, sender.clone())));
    drop(sender);
    (receiver, stdout_task, stderr_task)
}

fn abort_reader(task: Option<tokio::task::JoinHandle<()>>) {
    if let Some(task) = task {
        task.abort();
    }
}

async fn sleep_until_deadline(deadline: Option<Instant>) {
    if let Some(deadline) = deadline {
        tokio::time::sleep_until(tokio::time::Instant::from_std(deadline)).await;
    }
}

struct WaitState {
    output: ProcessOutput,
    exited: bool,
    stdout_ended: bool,
    stderr_ended: bool,
    deadline: Option<Instant>,
    max_bytes: Option<usize>,
    stopped: Option<CaptureStop>,
    error: Option<io::Error>,
    sink: OutputSink,
}

impl WaitState {
    fn new(stdout_ended: bool, stderr_ended: bool, max_bytes: Option<usize>, sink: OutputSink) -> Self {
        Self {
            output: ProcessOutput {
                status: None,
                stdout: Vec::new(),
                stderr: Vec::new(),
            },
            exited: false,
            stdout_ended,
            stderr_ended,
            deadline: None,
            max_bytes,
            stopped: None,
            error: None,
            sink,
        }
    }

    fn is_complete(&self) -> bool {
        self.exited && self.pipes_ended()
    }

    fn record_exit(&mut self, status: std::process::ExitStatus) {
        self.output.status = status.code();
        self.exited = true;
        if !self.pipes_ended() {
            self.arm_deadline();
        }
    }

    fn record_event(&mut self, event: Option<StreamEvent>) {
        match event {
            Some(StreamEvent::Data(stream, bytes)) => {
                let overflow = match &self.sink {
                    OutputSink::Capture => append(&mut self.output, stream, bytes, self.max_bytes),
                    OutputSink::Stream(on_data) => {
                        on_data(&bytes).unwrap_or_else(|error| crate::utils::uncaught::throw_callback_uncaught(error));
                        false
                    }
                };
                if overflow && self.stopped.is_none() {
                    self.stopped = Some(match stream {
                        Stream::Stdout => CaptureStop::StdoutLimit,
                        Stream::Stderr => CaptureStop::StderrLimit,
                    });
                }
                if self.exited {
                    self.arm_deadline();
                }
            }
            Some(StreamEvent::Failed(error)) => match &self.sink {
                OutputSink::Capture => self.error = Some(error),
                OutputSink::Stream(_) => {
                    crate::utils::uncaught::throw_native_uncaught(pi_js_compat::node_error::NativeNodeError::from(
                        pi_js_compat::node_fs::node_stream_error_record(&error, "read"),
                    ));
                }
            },
            Some(StreamEvent::Ended(Stream::Stdout)) => self.stdout_ended = true,
            Some(StreamEvent::Ended(Stream::Stderr)) => self.stderr_ended = true,
            None => {
                self.stdout_ended = true;
                self.stderr_ended = true;
            }
        }
    }

    fn pipes_ended(&self) -> bool {
        self.stdout_ended && self.stderr_ended
    }

    fn arm_deadline(&mut self) {
        self.deadline = Some(Instant::now() + Duration::from_millis(EXIT_STDIO_GRACE_MS));
    }
}

#[derive(Debug, Clone, Copy)]
enum Stream {
    Stdout,
    Stderr,
}

enum StreamEvent {
    Data(Stream, Vec<u8>),
    Ended(Stream),
    Failed(io::Error),
}

async fn read_stream(mut reader: impl AsyncRead + Unpin, stream: Stream, sender: mpsc::Sender<StreamEvent>) {
    let mut buffer = vec![0u8; READ_BUFFER_BYTES];
    loop {
        match reader.read(&mut buffer).await {
            Ok(0) => {
                let _ = sender.send(StreamEvent::Ended(stream)).await;
                return;
            }
            Ok(bytes_read) => {
                let chunk = buffer[..bytes_read].to_vec();
                if sender.send(StreamEvent::Data(stream, chunk)).await.is_err() {
                    return;
                }
            }
            Err(error) => {
                let _ = sender.send(StreamEvent::Failed(error)).await;
                return;
            }
        }
    }
}

fn append(output: &mut ProcessOutput, stream: Stream, bytes: Vec<u8>, max_bytes: Option<usize>) -> bool {
    let output = match stream {
        Stream::Stdout => &mut output.stdout,
        Stream::Stderr => &mut output.stderr,
    };
    let remaining = max_bytes
        .map(|limit| limit.saturating_sub(output.len()))
        .unwrap_or(bytes.len());
    let count = remaining.min(bytes.len());
    output.extend_from_slice(&bytes[..count]);
    count < bytes.len()
}

/// Node error text for a failed `spawn`: `spawn <command> <ERRNO>`.
///
/// Node reports the `uv_err_name` code, not the OS description, so the errno name is looked up
/// from the shared libuv table instead of using `io::Error`'s display text.
pub fn node_spawn_error_message(
    prefix: &str,
    command: &pi_js_compat::js_json::JsString,
    error: &io::Error,
) -> pi_js_compat::js_json::JsString {
    match error.raw_os_error() {
        Some(_) => pi_js_compat::js_format!("{} {} {}", prefix, command, node_error_code(error)),
        None => error.to_string().into(),
    }
}
