//! Frozen `host-relay-client.ts`: SSH stdio transport and bounded jittered reconnect.
use super::host_relay::{HostRelayOptions, run_host_relay};
use pi_js_compat::abort::AbortSignal;
use pi_js_compat::js_format;
use pi_js_compat::js_json::{JsString, encode_node_utf8};
use std::process::Stdio;
use std::time::Duration;
use tokio::io::AsyncReadExt;
use tokio::process::{Child, Command};

/// `RelaySettings` from the `relay` settings object; text stays UTF-16 until the ssh argv boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelaySettings {
    pub peer: JsString,
    pub remote_command: Option<JsString>,
}

/// The ssh argv; Node encodes each argument to UTF-8 at spawn.
pub fn ssh_arguments(settings: &RelaySettings) -> Vec<JsString> {
    let remote_command = settings
        .remote_command
        .clone()
        .unwrap_or_else(|| "pi relay serve".into());
    let mut arguments: Vec<JsString> = ["-o", "BatchMode=yes", "-o", "ServerAliveInterval=15"]
        .into_iter()
        .map(JsString::from)
        .collect();
    arguments.extend([settings.peer.clone(), remote_command]);
    arguments
}

/// `!settings.peer?.trim() || settings.peer.startsWith("-")`.
pub fn validate_peer(peer: &JsString) -> Result<(), JsString> {
    if peer.trim().is_empty() || peer.starts_with('-') {
        return Err("relay.peer must be a non-empty SSH alias, not an option".into());
    }
    Ok(())
}
pub fn reconnect_delay_ms(failures: u32, random: f64) -> u64 {
    let base = (1000u64 * 2u64.pow(failures.min(5))).min(30_000);
    (base as f64 * (0.5 + random * 0.5)).round() as u64
}
pub fn random_fraction() -> Result<f64, String> {
    let mut bytes = [0; 8];
    getrandom::fill(&mut bytes).map_err(|e| e.to_string())?;
    Ok((u64::from_ne_bytes(bytes) >> 11) as f64 / ((1u64 << 53) as f64))
}
/// `stderr = (stderr + chunk.toString()).slice(-4096)`: the tail is cut by UTF-16 units, so it
/// may start with a lone low surrogate exactly as in the source.
async fn read_stderr_tail(mut stderr: tokio::process::ChildStderr) -> Result<JsString, JsString> {
    let mut tail = JsString::default();
    let mut chunk = [0; 4096];
    loop {
        let count = stderr.read(&mut chunk).await.map_err(io_message)?;
        if count == 0 {
            return Ok(tail);
        }
        tail = tail + JsString::from(String::from_utf8_lossy(&chunk[..count]).as_ref());
        if tail.len() > 4096 {
            tail = tail.slice(tail.len() - 4096, tail.len());
        }
    }
}
fn io_message(error: impl ToString) -> JsString {
    error.to_string().into()
}
fn terminate(child: &Child) -> Result<(), JsString> {
    let Some(pid) = child.id().and_then(|pid| rustix::process::Pid::from_raw(pid as i32)) else {
        return Ok(());
    };
    match rustix::process::kill_process(pid, rustix::process::Signal::TERM) {
        Ok(()) | Err(rustix::io::Errno::SRCH) => Ok(()),
        Err(e) => Err(io_message(e)),
    }
}
async fn cleanup_child(child: &mut Child) -> Result<(), JsString> {
    if child.try_wait().map_err(io_message)?.is_some() {
        return Ok(());
    }
    terminate(child)?;
    match tokio::time::timeout(Duration::from_secs(1), child.wait()).await {
        Ok(status) => {
            status.map_err(io_message)?;
        }
        Err(_) => child.kill().await.map_err(io_message)?,
    }
    Ok(())
}
async fn connect_relay(path: &JsString, settings: &RelaySettings, signal: &AbortSignal) -> Result<(), JsString> {
    let mut child = Command::new("ssh")
        .args(ssh_arguments(settings).iter().map(encode_node_utf8))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| crate::utils::child_process::node_spawn_error_message("spawn", &"ssh".into(), &error))?;
    let stderr = tokio::spawn(read_stderr_tail(child.stderr.take().unwrap()));
    let result = run_host_relay(
        child.stdout.take().unwrap(),
        child.stdin.take().unwrap(),
        HostRelayOptions {
            control_db_path: path,
            host: None,
            poll_ms: None,
            signal: signal.clone(),
        },
    )
    .await;
    let status = match result {
        Ok(()) => wait_for_ssh_exit(&mut child, signal).await,
        Err(error) => Err(JsString::from(error)),
    };
    cleanup_child(&mut child).await?;
    let tail = stderr.await.map_err(io_message)??;
    check_ssh_status(status?, &tail, signal.aborted())
}

async fn wait_for_ssh_exit(child: &mut Child, signal: &AbortSignal) -> Result<std::process::ExitStatus, JsString> {
    tokio::select! {
        biased;
        _ = signal.wait() => { terminate(child)?; child.wait().await },
        status = child.wait() => status,
    }
    .map_err(io_message)
}
/// ``ssh exited ${code ?? exitSignal}: ${stderr.trim()}``.
fn check_ssh_status(status: std::process::ExitStatus, stderr: &JsString, cancelled: bool) -> Result<(), JsString> {
    if cancelled || status.code() == Some(0) {
        return Ok(());
    }
    let code = status
        .code()
        .map(|code| code.to_string())
        .unwrap_or_else(|| crate::utils::child_process::exit_signal_name(&status));
    Err(js_format!("ssh exited {}: {}", code, stderr.trim()))
}

/// Failure stays within this connection; local canonical queues remain untouched.
pub async fn run_ssh_relay_client(
    path: &JsString,
    settings: &RelaySettings,
    signal: AbortSignal,
) -> Result<(), String> {
    let mut failures = 0u32;
    while !signal.aborted() {
        let started = tokio::time::Instant::now();
        let result = match validate_peer(&settings.peer) {
            Ok(()) => connect_relay(path, settings, &signal).await,
            Err(e) => Err(e),
        };
        if signal.aborted() {
            break;
        }
        match result {
            // `console.error`: stderr is the UTF-8 boundary.
            Ok(()) => eprintln!(
                "[relay:{}] connection closed; reconnecting",
                encode_node_utf8(&settings.peer)
            ),
            Err(error) => eprintln!(
                "[relay:{}] {}",
                encode_node_utf8(&settings.peer),
                encode_node_utf8(&error)
            ),
        }
        if started.elapsed() >= Duration::from_secs(30) {
            failures = 0;
        }
        let delay = reconnect_delay_ms(failures, random_fraction()?);
        failures = failures.saturating_add(1);
        tokio::select! { _ = signal.wait() => break, _ = tokio::time::sleep(Duration::from_millis(delay)) => {} }
    }
    Ok(())
}
