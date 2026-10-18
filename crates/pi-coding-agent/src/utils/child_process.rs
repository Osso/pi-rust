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
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let (sender, mut receiver) = mpsc::channel(64);

    let stdout_task = stdout.map(|reader| tokio::spawn(read_stream(reader, Stream::Stdout, sender.clone())));
    let stderr_task = stderr.map(|reader| tokio::spawn(read_stream(reader, Stream::Stderr, sender.clone())));
    drop(sender);

    let mut output = ProcessOutput {
        status: None,
        stdout: Vec::new(),
        stderr: Vec::new(),
    };
    let mut stdout_ended = stdout_task.is_none();
    let mut stderr_ended = stderr_task.is_none();
    let mut exited = false;
    let mut deadline: Option<Instant> = None;
    let grace = Duration::from_millis(EXIT_STDIO_GRACE_MS);

    let wait = child.wait();
    tokio::pin!(wait);

    loop {
        if exited && stdout_ended && stderr_ended {
            break;
        }

        let idle = async {
            let deadline = deadline.unwrap_or_else(Instant::now);
            tokio::time::sleep_until(tokio::time::Instant::from_std(deadline)).await;
        };

        tokio::select! {
            status = &mut wait, if !exited => {
                let status = status?;
                output.status = status.code();
                exited = true;
                if !(stdout_ended && stderr_ended) {
                    deadline = Some(Instant::now() + grace);
                }
            }
            event = receiver.recv() => {
                match event {
                    Some(StreamEvent::Data(stream, bytes)) => {
                        append(&mut output, stream, bytes);
                        if exited {
                            deadline = Some(Instant::now() + grace);
                        }
                    }
                    Some(StreamEvent::Ended(Stream::Stdout)) => stdout_ended = true,
                    Some(StreamEvent::Ended(Stream::Stderr)) => stderr_ended = true,
                    None => {
                        stdout_ended = true;
                        stderr_ended = true;
                    }
                }
            }
            () = idle, if deadline.is_some() => break,
        }
    }

    if let Some(task) = stdout_task {
        task.abort();
    }
    if let Some(task) = stderr_task {
        task.abort();
    }

    Ok(output)
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
