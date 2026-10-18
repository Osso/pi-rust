use std::io;
use std::path::Path;
use std::process::Stdio;
use std::time::{Duration, Instant};

use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::{Child, Command};
use tokio::sync::mpsc;

pub const EXIT_STDIO_GRACE_MS: u64 = 100;

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
    let (mut receiver, stdout_task, stderr_task) = start_output_readers(child);
    let mut state = WaitState::new(stdout_task.is_none(), stderr_task.is_none());

    let wait = child.wait();
    tokio::pin!(wait);

    loop {
        if state.is_complete() {
            break;
        }
        let idle = sleep_until_deadline(state.deadline);
        tokio::select! {
            status = &mut wait, if !state.exited => state.record_exit(status?),
            event = receiver.recv() => state.record_event(event),
            () = idle, if state.deadline.is_some() => break,
        }
    }

    abort_reader(stdout_task);
    abort_reader(stderr_task);
    Ok(state.output)
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
}

impl WaitState {
    fn new(stdout_ended: bool, stderr_ended: bool) -> Self {
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
                append(&mut self.output, stream, bytes);
                if self.exited {
                    self.arm_deadline();
                }
            }
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
}

async fn read_stream(mut reader: impl AsyncRead + Unpin, stream: Stream, sender: mpsc::Sender<StreamEvent>) {
    let mut buffer = vec![0u8; 8192];
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
            Err(_) => {
                let _ = sender.send(StreamEvent::Ended(stream)).await;
                return;
            }
        }
    }
}

fn append(output: &mut ProcessOutput, stream: Stream, bytes: Vec<u8>) {
    match stream {
        Stream::Stdout => output.stdout.extend(bytes),
        Stream::Stderr => output.stderr.extend(bytes),
    }
}
