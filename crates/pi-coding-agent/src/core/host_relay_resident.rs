//! Frozen `host-relay-resident.ts`: versioned process identity, proper-lockfile and detached CLI.
use super::host_relay_client::{RelaySettings, random_fraction, run_ssh_relay_client};
use super::runtime_process::{
    ProcessIdentity, is_process_identity_alive, is_verified_pi_runtime_process, read_process_identity,
};
use crate::utils::lockfile::{self, LockErrorCode, LockGuard, LockOptions};
use pi_js_compat::js_json::{JsObject, JsString, JsValue};
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RelayIdentity {
    pub pid: u32,
    pub start_time_ticks: u64,
    pub version: String,
}
pub fn read_relay_identity(path: &str) -> Result<Option<RelayIdentity>, String> {
    let identity_path = format!("{path}.relay.json");
    let raw = match fs::read_to_string(&identity_path) {
        Ok(raw) => raw,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.to_string()),
    };
    let value = pi_js_compat::js_json::parse_lossless(&raw).map_err(|error| error.to_string())?;
    let pid = value.get("pid").and_then(pi_js_compat::js_json::JsValue::as_f64);
    let ticks = value
        .get("startTimeTicks")
        .and_then(pi_js_compat::js_json::JsValue::as_f64);
    let version = value.get("version").and_then(pi_js_compat::js_json::JsValue::as_string);
    let (Some(pid), Some(ticks), Some(version)) = (pid, ticks, version) else {
        return Err(format!("Invalid relay identity in {identity_path}"));
    };
    let valid_pid = pid.fract() == 0.0 && pid >= 1.0 && pid <= u32::MAX as f64;
    let valid_ticks = ticks.fract() == 0.0 && ticks >= 1.0 && ticks <= u64::MAX as f64;
    if !valid_pid || !valid_ticks {
        return Ok(None);
    }
    let process = ProcessIdentity {
        pid: pid as u32,
        start_time_ticks: ticks as u64,
        incarnation: None,
    };
    Ok(is_process_identity_alive(&process).then(|| RelayIdentity {
        pid: process.pid,
        start_time_ticks: process.start_time_ticks,
        version: version.to_utf8_lossy(),
    }))
}
fn create_private_parent(path: &str) -> Result<(), String> {
    if let Some(parent) = Path::new(path).parent() {
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(parent)
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}
async fn acquire_relay_lock(path: &str, startup: bool) -> Result<Option<LockGuard>, String> {
    let suffix = if startup { ".relay-start.lock" } else { ".relay.lock" };
    let options = LockOptions {
        realpath: false,
        stale_ms: 5000,
        update_ms: Some(1000),
        lockfile_path: Some(PathBuf::from(format!("{path}{suffix}"))),
        ..Default::default()
    };
    let retries = if startup { 100 } else { 60 };
    let mut delays = (0..retries)
        .map(|attempt| {
            if startup {
                Ok((25.0 * 1.1f64.powi(attempt) * (1.0 + random_fraction()?))
                    .round()
                    .min(100.0) as u64)
            } else {
                Ok(100)
            }
        })
        .collect::<Result<Vec<_>, String>>()?;
    delays.sort_unstable();
    for delay in delays.into_iter().map(Some).chain(std::iter::once(None)) {
        match lockfile::acquire(Path::new(path), &options).await {
            Ok(guard) => return Ok(Some(guard)),
            Err(e) if e.code == LockErrorCode::Locked && delay.is_some() => {
                tokio::time::sleep(Duration::from_millis(delay.unwrap())).await
            }
            Err(e) if !startup && e.code == LockErrorCode::Locked => return Ok(None),
            Err(e) => return Err(e.to_string()),
        }
    }
    unreachable!("bounded acquisition returns on final attempt")
}
async fn wait_for_relay(path: &str, ready: bool) -> Result<(), String> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    while tokio::time::Instant::now() < deadline {
        let identity = read_relay_identity(path)?;
        if if ready {
            identity
                .as_ref()
                .is_some_and(|identity| identity.version == crate::config::version())
        } else {
            identity.is_none()
        } {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    Err(format!(
        "Relay did not {} within 10000ms",
        if ready { "start" } else { "stop" }
    ))
}
fn terminate_resident(identity: &RelayIdentity) -> Result<(), String> {
    if identity.pid == std::process::id() || !is_verified_pi_runtime_process(identity.pid) {
        return Err(format!(
            "Refusing to terminate unverified relay process {}",
            identity.pid
        ));
    }
    let pid = rustix::process::Pid::from_raw(identity.pid as i32).ok_or_else(|| "Invalid relay pid".to_owned())?;
    rustix::process::kill_process(pid, rustix::process::Signal::TERM).map_err(|e| e.to_string())
}
fn launch_relay(path: &str) -> Result<(), String> {
    let log = OpenOptions::new()
        .append(true)
        .create(true)
        .mode(0o600)
        .open(format!("{path}.relay.log"))
        .map_err(|e| e.to_string())?;
    let executable = std::env::current_exe().map_err(|e| e.to_string())?;
    let home = pi_js_compat::process_env::get("HOME").unwrap_or_default();
    // A native binary is the frozen Bun-binary branch: no JS loader arguments.
    let child = Command::new("setsid")
        .arg(executable)
        .args(["relay", "connect"])
        .current_dir(home)
        .envs(pi_js_compat::process_env::vars())
        .stdin(Stdio::null())
        .stdout(log.try_clone().map_err(|e| e.to_string())?)
        .stderr(log)
        .spawn()
        .map_err(|e| e.to_string())?;
    // Reap without retaining the startup future or keeping the event loop alive.
    std::thread::spawn(move || {
        let mut child = child;
        if let Err(error) = child.wait() {
            eprintln!("Relay process error: {error}");
        }
    });
    Ok(())
}
/// Called in a detached startup task; relay failure must not fail its session.
pub async fn ensure_relay_running(path: &JsString, settings: Option<&RelaySettings>) -> Result<(), String> {
    if settings.is_none() {
        return Ok(());
    }
    // Every use below is a filesystem path (directory, lock, identity, log).
    let path = &pi_js_compat::js_json::encode_node_utf8(path);
    create_private_parent(path)?;
    let guard = acquire_relay_lock(path, true)
        .await?
        .expect("startup acquisition returns a lock or error");
    let result = async {
        let resident = read_relay_identity(path)?;
        if resident
            .as_ref()
            .is_some_and(|identity| identity.version == crate::config::version())
        {
            return Ok(());
        }
        if let Some(resident) = resident {
            terminate_resident(&resident)?;
            wait_for_relay(path, false).await?;
        }
        launch_relay(path)?;
        wait_for_relay(path, true).await
    }
    .await;
    guard.release().map_err(|e| e.to_string())?;
    result
}
/// `getGlobalSettings().relay`. The source casts the value; a non-object relay or a non-string
/// `peer`/`remoteCommand` is an explicit error here (invalid JavaScript runtime input types).
pub fn relay_settings_from_global(settings: &JsObject) -> Result<Option<RelaySettings>, String> {
    let relay = match settings.get("relay") {
        None | Some(JsValue::Null) => return Ok(None),
        Some(JsValue::Object(relay)) => relay,
        Some(_) => return Err("relay settings must be an object".into()),
    };
    let peer = match relay.get("peer") {
        Some(JsValue::String(peer)) => peer.clone(),
        _ => return Err("relay.peer must be a string".into()),
    };
    let remote_command = match relay.get("remoteCommand") {
        None => None,
        Some(JsValue::String(command)) => Some(command.clone()),
        Some(_) => return Err("relay.remoteCommand must be a string".into()),
    };
    Ok(Some(RelaySettings { peer, remote_command }))
}
pub async fn run_relay_connect() -> Result<(), pi_ai::request::CallbackError> {
    let home = JsString::from(pi_js_compat::process_env::get("HOME").unwrap_or_default());
    let agent_dir = JsString::from(crate::config::get_agent_dir()?.display().to_string());
    let settings = super::settings_manager::SettingsManager::create(
        &home,
        Some(&agent_dir),
        super::settings_manager::SettingsManagerCreateOptions {
            project_trusted: Some(false),
        },
    )
    .map_err(pi_ai::request::CallbackError::from)?
    .get_global_settings();
    let Some(settings) = relay_settings_from_global(&settings)? else {
        return Ok(());
    };
    let path = super::session_control_db::get_control_db_path(&crate::config::get_user_state_root()?);
    create_private_parent(&path)?;
    let Some(guard) = acquire_relay_lock(&path, false).await? else {
        return Ok(());
    };
    let controller = pi_js_compat::abort::AbortController::new();
    let signal = controller.signal();
    let signals = spawn_signal_listener(controller.clone())?;
    let result = async {
        let process = read_process_identity(std::process::id()).map_err(|e| e.to_string())?;
        write_relay_identity(
            &path,
            &RelayIdentity {
                pid: process.pid,
                start_time_ticks: process.start_time_ticks,
                version: crate::config::version().to_owned(),
            },
        )?;
        run_ssh_relay_client(&JsString::from(path.as_str()), &settings, signal.clone()).await
    }
    .await;
    controller.abort(None);
    signals.abort();
    let removed = fs::remove_file(format!("{path}.relay.json")).map_err(|e| e.to_string());
    guard.release().map_err(|e| e.to_string())?;
    removed?;
    result.map_err(Into::into)
}
pub fn write_relay_identity(path: &str, identity: &RelayIdentity) -> Result<(), String> {
    use std::io::Write;
    let filename = format!("{path}.relay.json");
    let wire = pi_js_compat::js_json::stringify(&serde_json::to_value(identity).map_err(|e| e.to_string())?);
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(format!("{filename}.tmp"))
        .map_err(|e| e.to_string())?;
    file.write_all(wire.as_bytes()).map_err(|e| e.to_string())?;
    fs::rename(format!("{filename}.tmp"), filename).map_err(|e| e.to_string())
}
fn spawn_signal_listener(
    controller: pi_js_compat::abort::AbortController,
) -> Result<tokio::task::JoinHandle<()>, String> {
    use tokio::signal::unix::{SignalKind, signal as listen};
    let mut term = listen(SignalKind::terminate()).map_err(|e| e.to_string())?;
    let mut interrupt = listen(SignalKind::interrupt()).map_err(|e| e.to_string())?;
    Ok(tokio::spawn(async move {
        tokio::select! { _ = term.recv() => {}, _ = interrupt.recv() => {} }
        controller.abort(None);
    }))
}
