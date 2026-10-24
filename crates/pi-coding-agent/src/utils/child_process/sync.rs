//! Synchronous Node child-process capture. One runner for all sync consumers.

use std::io::{self, Read};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::time::{Duration, Instant};

use pi_js_compat::js_json::{JsString, JsValue, encode_node_utf8};
use pi_js_compat::node_error::NativeNodeError;

const READ_BYTES: usize = 65_536;
const EXIT_POLL_INTERVAL: Duration = Duration::from_millis(10);

/// Typed options; `None` leaves native capture unbounded (existing caller policy).
/// Clipboard supplies the frozen explicit timeout and buffer limits.
pub struct SpawnSyncOptions<'a> {
    pub cwd: Option<&'a Path>,
    pub timeout: Option<Duration>,
    pub max_buffer: Option<usize>,
    pub kill_signal: rustix::process::Signal,
}

impl Default for SpawnSyncOptions<'_> {
    fn default() -> Self {
        Self {
            cwd: None,
            timeout: None,
            max_buffer: None,
            kill_signal: rustix::process::Signal::TERM,
        }
    }
}

/// Node `spawnSync`: failed spawn has null pipes/status and pid0; runtime failures
/// retain captured pipes/status and add the original errno-bearing Error.
#[derive(Debug)]
pub struct SpawnSyncResult {
    pub pid: u32,
    pub status: Option<i32>,
    pub signal: Option<String>,
    pub stdout: Option<Vec<u8>>,
    pub stderr: Option<Vec<u8>>,
    pub error: Option<NativeNodeError>,
}

impl SpawnSyncResult {
    fn failed(error: NativeNodeError) -> Self {
        Self {
            pid: 0,
            status: None,
            signal: None,
            stdout: None,
            stderr: None,
            error: Some(error),
        }
    }
}

/// Validation throws; OS spawn/capture failures are `result.error`, like Node.
pub fn spawn_sync(
    program: &JsString,
    arguments: &[JsString],
    options: SpawnSyncOptions<'_>,
) -> Result<SpawnSyncResult, Box<NativeNodeError>> {
    pi_js_compat::node_child_process::validate_spawn_arguments(program, arguments)?;
    if let Some(cwd) = options.cwd {
        pi_js_compat::node_child_process::validate_string_argument(
            "options.cwd",
            "must be a string without null bytes",
            &pi_js_compat::node_fs::decode_node_path(cwd.as_os_str()),
        )?;
    }
    let mut command = Command::new(encode_node_utf8(program));
    command
        .args(arguments.iter().map(encode_node_utf8))
        .env_clear()
        .envs(pi_js_compat::process_env::vars())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(cwd) = options.cwd {
        command.current_dir(cwd);
    }
    let child = match command.spawn() {
        Ok(child) => child,
        Err(error) => return Ok(SpawnSyncResult::failed(spawn_error(program, arguments, &error))),
    };
    Ok(capture_sync_child(child, program, arguments, options))
}

#[derive(Clone, Copy)]
enum Pipe {
    Stdout,
    Stderr,
}

enum PipeEvent {
    Data(Pipe, Vec<u8>),
    End,
    Error(io::Error),
}

fn read_pipe(mut pipe: impl Read, kind: Pipe, sender: SyncSender<PipeEvent>) {
    let mut buffer = [0; READ_BYTES];
    loop {
        match pipe.read(&mut buffer) {
            Ok(0) => {
                let _ = sender.send(PipeEvent::End);
                return;
            }
            Ok(count) => {
                if sender.send(PipeEvent::Data(kind, buffer[..count].to_vec())).is_err() {
                    return;
                }
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => {
                let _ = sender.send(PipeEvent::Error(error));
                return;
            }
        }
    }
}

struct SyncCapture {
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    ended: usize,
    failure: Option<io::Error>,
    deadline: Option<Instant>,
}

impl SyncCapture {
    fn record(&mut self, event: PipeEvent, max_buffer: Option<usize>) {
        match event {
            PipeEvent::End => self.ended += 1,
            PipeEvent::Error(error) => {
                self.ended += 1;
                if self.failure.is_none() {
                    self.failure = Some(error);
                }
            }
            PipeEvent::Data(kind, bytes) if self.failure.is_none() => {
                match kind {
                    Pipe::Stdout => self.stdout.extend(bytes),
                    Pipe::Stderr => self.stderr.extend(bytes),
                }
                if max_buffer.is_some_and(|limit| self.stdout.len().saturating_add(self.stderr.len()) > limit) {
                    self.failure = Some(io::Error::from_raw_os_error(rustix::io::Errno::NOBUFS.raw_os_error()));
                }
            }
            PipeEvent::Data(_, _) => {}
        }
    }

    fn poll_timeout(&mut self) {
        if self.failure.is_none() && self.deadline.is_some_and(|deadline| Instant::now() >= deadline) {
            self.failure = Some(io::Error::from_raw_os_error(rustix::io::Errno::TIMEDOUT.raw_os_error()));
        }
    }
}

fn capture_sync_child(
    mut child: Child,
    program: &JsString,
    arguments: &[JsString],
    options: SpawnSyncOptions<'_>,
) -> SpawnSyncResult {
    let pid = child.id();
    let (sender, receiver) = mpsc::sync_channel(16);
    let stdout = child.stdout.take().expect("sync stdout is piped");
    let stderr = child.stderr.take().expect("sync stderr is piped");
    std::thread::scope(|scope| {
        let stdout_sender = sender.clone();
        scope.spawn(move || read_pipe(stdout, Pipe::Stdout, stdout_sender));
        scope.spawn(move || read_pipe(stderr, Pipe::Stderr, sender));
        let mut capture = SyncCapture {
            stdout: Vec::new(),
            stderr: Vec::new(),
            ended: 0,
            failure: None,
            deadline: options
                .timeout
                .filter(|timeout| !timeout.is_zero())
                .and_then(|timeout| Instant::now().checked_add(timeout)),
        };
        let status = wait_sync_child(&mut child, &receiver, &mut capture, &options);
        SpawnSyncResult {
            pid,
            status: status.as_ref().and_then(std::process::ExitStatus::code),
            signal: status
                .as_ref()
                .filter(|status| status.code().is_none())
                .map(super::exit_signal_name),
            stdout: Some(capture.stdout),
            stderr: Some(capture.stderr),
            error: capture.failure.map(|error| spawn_error(program, arguments, &error)),
        }
    })
}

fn wait_sync_child(
    child: &mut Child,
    receiver: &Receiver<PipeEvent>,
    capture: &mut SyncCapture,
    options: &SpawnSyncOptions<'_>,
) -> Option<std::process::ExitStatus> {
    let mut status = None;
    let mut killed = false;
    loop {
        if status.is_none() {
            match child.try_wait() {
                Ok(value) => status = value,
                Err(error) => {
                    capture.failure = Some(error);
                    break;
                }
            }
        }
        if status.is_some() && capture.ended == 2 {
            return status;
        }
        capture.poll_timeout();
        if capture.failure.is_some() && !killed {
            if status.is_none()
                && let Err(error) = signal_sync_child(child, options.kill_signal)
            {
                capture.failure = Some(error);
            }
            killed = true;
        }
        let wait = capture
            .deadline
            .filter(|_| !killed)
            .map_or(EXIT_POLL_INTERVAL, |deadline| {
                deadline
                    .saturating_duration_since(Instant::now())
                    .min(EXIT_POLL_INTERVAL)
            });
        match receiver.recv_timeout(wait) {
            Ok(event) => capture.record(event, options.max_buffer),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => std::thread::sleep(wait),
        }
    }
    // A wait failure is explicit, but still reap the child if the OS permits it.
    if let Err(error) = signal_sync_child(child, options.kill_signal) {
        capture.failure = Some(error);
    }
    match child.wait() {
        Ok(status) => Some(status),
        Err(error) => {
            capture.failure = Some(error);
            None
        }
    }
}

fn signal_sync_child(child: &Child, signal: rustix::process::Signal) -> io::Result<()> {
    let pid = rustix::process::Pid::from_raw(child.id() as i32).expect("live child PID is positive");
    match rustix::process::kill_process(pid, signal) {
        Ok(()) | Err(rustix::io::Errno::SRCH) => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn spawn_error(program: &JsString, arguments: &[JsString], error: &io::Error) -> NativeNodeError {
    let code = super::node_error_code(error);
    let syscall = pi_js_compat::js_format!("spawnSync {}", program);
    let mut value = NativeNodeError::new("Error", pi_js_compat::js_format!("{} {}", syscall, code));
    if let Some(errno) = error.raw_os_error() {
        value = value.with_property("errno", JsValue::Number(-f64::from(errno)));
    }
    value
        .with_property("code", JsValue::String(code.into()))
        .with_property("syscall", JsValue::String(syscall))
        .with_property("path", JsValue::String(program.clone()))
        .with_property(
            "spawnargs",
            JsValue::Array(arguments.iter().cloned().map(JsValue::String).collect()),
        )
}
