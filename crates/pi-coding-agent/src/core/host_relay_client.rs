//! Frozen `host-relay-client.ts`: SSH stdio transport and bounded jittered reconnect.
use super::host_relay::{HostRelayOptions, run_host_relay};
use serde::{Deserialize, Serialize};
use std::process::Stdio;
use std::time::Duration;
use tokio::io::AsyncReadExt;
use tokio::process::{Child, Command};
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RelaySettings {
    pub peer: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_command: Option<String>,
}
pub fn ssh_arguments(settings: &RelaySettings) -> Vec<String> {
    [
        "-o",
        "BatchMode=yes",
        "-o",
        "ServerAliveInterval=15",
        &settings.peer,
        settings.remote_command.as_deref().unwrap_or("pi relay serve"),
    ]
    .into_iter()
    .map(str::to_owned)
    .collect()
}
pub fn validate_peer(peer: &str) -> Result<(), String> {
    if pi_js_compat::string::js_trim(peer).is_empty() || peer.starts_with('-') {
        return Err("relay.peer must be a non-empty SSH alias, not an option".into());
    }
    Ok(())
}
pub fn reconnect_delay_ms(failures: u32, random: f64) -> u64 {
    let base = (1000u64 * 2u64.pow(failures.min(5))).min(30_000);
    (base as f64 * (0.5 + random * 0.5)).round() as u64
}
pub(crate) fn random_fraction() -> Result<f64, String> {
    let mut bytes = [0; 8];
    getrandom::fill(&mut bytes).map_err(|e| e.to_string())?;
    Ok((u64::from_ne_bytes(bytes) >> 11) as f64 / ((1u64 << 53) as f64))
}
async fn read_stderr_tail(mut stderr: tokio::process::ChildStderr) -> Result<String, String> {
    let mut tail = Vec::new();
    let mut chunk = [0; 4096];
    loop {
        let count = stderr.read(&mut chunk).await.map_err(|e| e.to_string())?;
        if count == 0 {
            return Ok(String::from_utf16_lossy(&tail));
        }
        tail.extend(String::from_utf8_lossy(&chunk[..count]).encode_utf16());
        if tail.len() > 4096 {
            tail.drain(..tail.len() - 4096);
        }
    }
}
fn terminate(child: &Child) -> Result<(), String> {
    let Some(pid) = child.id().and_then(|pid| rustix::process::Pid::from_raw(pid as i32)) else {
        return Ok(());
    };
    match rustix::process::kill_process(pid, rustix::process::Signal::TERM) {
        Ok(()) | Err(rustix::io::Errno::SRCH) => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}
async fn cleanup_child(child: &mut Child) -> Result<(), String> {
    if child.try_wait().map_err(|e| e.to_string())?.is_some() {
        return Ok(());
    }
    terminate(child)?;
    match tokio::time::timeout(Duration::from_secs(1), child.wait()).await {
        Ok(status) => {
            status.map_err(|error| error.to_string())?;
        }
        Err(_) => child.kill().await.map_err(|error| error.to_string())?,
    }
    Ok(())
}
async fn connect_relay(
    path: &str,
    settings: &RelaySettings,
    signal: &CancellationToken,
) -> Result<(), pi_js_compat::js_json::JsString> {
    let mut child = Command::new("ssh")
        .args(ssh_arguments(settings))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| {
            crate::utils::child_process::node_spawn_error_message(
                "spawn",
                &pi_js_compat::js_json::JsString::from("ssh"),
                &error,
            )
        })?;
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
        Err(error) => Err(error),
    };
    cleanup_child(&mut child).await?;
    let tail = stderr.await.map_err(|e| e.to_string())??;
    check_ssh_status(status?, &tail, signal.is_cancelled())
}

async fn wait_for_ssh_exit(child: &mut Child, signal: &CancellationToken) -> Result<std::process::ExitStatus, String> {
    tokio::select! {
        biased;
        _ = signal.cancelled() => { terminate(child)?; child.wait().await },
        status = child.wait() => status,
    }
    .map_err(|e| e.to_string())
}
fn check_ssh_status(status: std::process::ExitStatus, stderr: &str, cancelled: bool) -> Result<(), String> {
    if cancelled || status.code() == Some(0) {
        return Ok(());
    }
    let code = status
        .code()
        .map(|code| code.to_string())
        .unwrap_or_else(|| crate::utils::child_process::exit_signal_name(&status));
    Err(format!("ssh exited {code}: {}", pi_js_compat::string::js_trim(stderr)))
}

/// Failure stays within this connection; local canonical queues remain untouched.
pub async fn run_ssh_relay_client(
    path: &str,
    settings: &RelaySettings,
    signal: CancellationToken,
) -> Result<(), String> {
    let mut failures = 0u32;
    while !signal.is_cancelled() {
        let started = tokio::time::Instant::now();
        let result = match validate_peer(&settings.peer) {
            Ok(()) => connect_relay(path, settings, &signal).await,
            Err(e) => Err(e),
        };
        if signal.is_cancelled() {
            break;
        }
        match result {
            Ok(()) => eprintln!("[relay:{}] connection closed; reconnecting", settings.peer),
            Err(error) => eprintln!("[relay:{}] {error}", settings.peer),
        }
        if started.elapsed() >= Duration::from_secs(30) {
            failures = 0;
        }
        let delay = reconnect_delay_ms(failures, random_fraction()?);
        failures = failures.saturating_add(1);
        tokio::select! { _ = signal.cancelled() => break, _ = tokio::time::sleep(Duration::from_millis(delay)) => {} }
    }
    Ok(())
}
