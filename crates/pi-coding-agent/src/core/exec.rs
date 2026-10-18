//! Shared command execution (`core/exec.ts`).

use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use rustix::process::{Pid, Signal, kill_process};
use tokio::process::Command;
use tokio_util::sync::CancellationToken;

use crate::utils::child_process::wait_for_child_process;

const SIGKILL_DELAY: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Default)]
pub struct ExecOptions {
    pub signal: Option<CancellationToken>,
    /// Milliseconds; `None` or `0` disables the timeout.
    pub timeout: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecResult {
    pub stdout: String,
    pub stderr: String,
    pub code: i32,
    pub killed: bool,
}

pub async fn exec_command(command: &str, args: &[&str], cwd: &str, options: &ExecOptions) -> ExecResult {
    let spawned = Command::new(command)
        .args(args)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();
    let mut child = match spawned {
        Ok(child) => child,
        Err(_) => return failed_exec_result(),
    };
    let killed = Arc::new(AtomicBool::new(false));
    let terminator = start_terminator(child.id(), killed.clone(), options);

    let output = wait_for_child_process(&mut child).await;
    if let Some(terminator) = terminator {
        terminator.abort();
    }
    match output {
        Ok(output) => ExecResult {
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            code: output.status.unwrap_or(0),
            killed: killed.load(Ordering::SeqCst),
        },
        Err(_) => failed_exec_result(),
    }
}

fn failed_exec_result() -> ExecResult {
    ExecResult {
        stdout: String::new(),
        stderr: String::new(),
        code: 1,
        killed: false,
    }
}

/// Aborts (or times out) the child with SIGTERM, escalating to SIGKILL only when
/// SIGTERM fails, matching `proc.kill()` and the source's `!proc.killed` guard.
fn start_terminator(
    pid: Option<u32>,
    killed: Arc<AtomicBool>,
    options: &ExecOptions,
) -> Option<tokio::task::JoinHandle<()>> {
    let pid = pid?;
    let signal = options.signal.clone();
    if signal.as_ref().is_some_and(CancellationToken::is_cancelled) {
        terminate(pid, &killed);
        return None;
    }
    let timeout = options.timeout.filter(|timeout| *timeout > 0);
    if signal.is_none() && timeout.is_none() {
        return None;
    }
    Some(tokio::spawn(async move {
        match (signal, timeout) {
            (Some(signal), Some(timeout)) => {
                tokio::select! {
                    () = signal.cancelled() => {}
                    () = tokio::time::sleep(Duration::from_millis(timeout)) => {}
                }
            }
            (Some(signal), None) => signal.cancelled().await,
            (None, Some(timeout)) => tokio::time::sleep(Duration::from_millis(timeout)).await,
            (None, None) => return,
        }
        terminate(pid, &killed);
    }))
}

fn terminate(pid: u32, killed: &AtomicBool) {
    let Ok(pid) = i32::try_from(pid).ok().and_then(Pid::from_raw).ok_or(()) else {
        return;
    };
    if kill_process(pid, Signal::TERM).is_ok() {
        killed.store(true, Ordering::SeqCst);
        return;
    }
    std::thread::spawn(move || {
        std::thread::sleep(SIGKILL_DELAY);
        let _ = kill_process(pid, Signal::KILL);
    });
}
