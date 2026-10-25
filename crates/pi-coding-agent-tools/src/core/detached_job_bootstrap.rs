//! Gated detached payload bootstrap (`core/detached-job-bootstrap.ts`).
//!
//! Node uses `spawn(..., { detached: true })` plus a pipe on fd 3. `unsafe_code`
//! is forbidden, so the same process shape comes from `setsid(1)` and a shell
//! redirect: stdin is the gate pipe, fd 3 is a copy of it, and the payload's
//! stdin is `/dev/null` — identical descriptors, identical pid, new session.

use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;
use std::process::{Child, Command, ExitStatus, Stdio};

use crate::utils::paths::node_dirname;
use pi_js_compat::js_format;
use pi_js_compat::js_json::{JsString, encode_node_utf8};
use serde::Serialize;

use crate::core::runtime_process::{read_process_group_id, read_process_identity};

/// `exec 3<&0 </dev/null` reproduces the source's fd-3 gate on stdin.
const PAYLOAD_GATE_SCRIPT: &str = r#"exec 3<&0 </dev/null; IFS= read -r _ <&3 || exit 125; exec "$@""#;

#[derive(Debug)]
pub enum DetachedPayloadError {
    Io(io::Error),
    IdentityNotPersisted,
    AlreadyReleased,
    SplitOutput,
    SpawnFailed(JsString),
}

impl DetachedPayloadError {
    pub fn message(&self) -> JsString {
        match self {
            Self::Io(source) => crate::core::tools::node_fs::operation_error_message(source),
            Self::IdentityNotPersisted => "Detached payload identity must be persisted before release".into(),
            Self::AlreadyReleased => "Detached payload gate was already released".into(),
            Self::SplitOutput => "Split detached payload output is not implemented".into(),
            Self::SpawnFailed(command) => js_format!("Could not start detached payload: {}", command),
        }
    }
}
impl std::fmt::Display for DetachedPayloadError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&encode_node_utf8(&self.message()))
    }
}

impl std::error::Error for DetachedPayloadError {}

impl From<io::Error> for DetachedPayloadError {
    fn from(source: io::Error) -> Self {
        Self::Io(source)
    }
}

pub struct GatedDetachedPayloadInput<'a> {
    pub args: &'a [JsString],
    pub command: &'a JsString,
    pub cwd: &'a JsString,
    /// `None` inherits the parent environment.
    pub env: Option<&'a [(JsString, JsString)]>,
    pub identity_path: &'a JsString,
    pub stderr_path: &'a JsString,
    pub stdout_path: &'a JsString,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GatedDetachedPayloadExit {
    pub exit_code: Option<i32>,
    pub signal: Option<i32>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PayloadIdentity {
    pgid: u64,
    pid: u32,
    start_time_ticks: u64,
}

#[derive(Debug)]
pub struct GatedDetachedPayload {
    child: Child,
    pid: u32,
    identity_path: JsString,
    gate: Option<std::process::ChildStdin>,
    identity_persisted: bool,
    released: bool,
}

pub fn spawn_gated_detached_payload(
    input: &GatedDetachedPayloadInput<'_>,
) -> Result<GatedDetachedPayload, DetachedPayloadError> {
    let output = open_shared_output_file(input.stdout_path, input.stderr_path)?;
    let setsid = find_setsid().ok_or_else(|| DetachedPayloadError::SpawnFailed(input.command.to_owned()))?;
    let mut command = Command::new(setsid);
    command
        .arg("/bin/sh")
        .arg("-c")
        .arg(PAYLOAD_GATE_SCRIPT)
        .arg("pi-detached-gate")
        .arg(encode_node_utf8(input.command))
        .args(input.args.iter().map(encode_node_utf8))
        .current_dir(encode_node_utf8(input.cwd))
        .stdin(Stdio::piped())
        .stdout(Stdio::from(output.try_clone()?))
        .stderr(Stdio::from(output));
    command.env_clear();
    match input.env {
        Some(environment) => command.envs(
            environment
                .iter()
                .map(|(name, value)| (encode_node_utf8(name), encode_node_utf8(value))),
        ),
        None => command.envs(pi_js_compat::process_env::vars()),
    };
    let mut child = command
        .spawn()
        .map_err(|_| DetachedPayloadError::SpawnFailed(input.command.to_owned()))?;
    let pid = child.id();
    let gate = child.stdin.take().expect("the gate pipe is always piped");
    Ok(GatedDetachedPayload {
        child,
        pid,
        identity_path: input.identity_path.clone(),
        gate: Some(gate),
        identity_persisted: false,
        released: false,
    })
}

impl GatedDetachedPayload {
    pub fn pid(&self) -> u32 {
        self.pid
    }

    pub fn persist_identity(&mut self) -> Result<(), DetachedPayloadError> {
        let identity = read_process_identity(self.pid)
            .map_err(|source| io::Error::other(crate::core::extensions::types::HandlerError::new(source.message)))?;
        let identity = PayloadIdentity {
            pgid: read_process_group_id(self.pid).map_err(|source| {
                io::Error::other(crate::core::extensions::types::HandlerError::new(source.message))
            })?,
            pid: identity.pid,
            start_time_ticks: identity.start_time_ticks,
        };
        write_identity_file(&self.identity_path, &identity)?;
        self.identity_persisted = true;
        Ok(())
    }

    pub fn release(&mut self) -> Result<(), DetachedPayloadError> {
        if !self.identity_persisted {
            return Err(DetachedPayloadError::IdentityNotPersisted);
        }
        if self.released {
            return Err(DetachedPayloadError::AlreadyReleased);
        }
        self.released = true;
        if let Some(mut gate) = self.gate.take() {
            gate.write_all(b"run\n")?;
        }
        Ok(())
    }

    pub async fn wait_for_exit(self) -> io::Result<GatedDetachedPayloadExit> {
        let child = self.child;
        let status = tokio::task::spawn_blocking(move || {
            let mut child = child;
            child.wait()
        })
        .await
        .map_err(io::Error::other)??;
        Ok(exit_of(status))
    }
}

fn exit_of(status: ExitStatus) -> GatedDetachedPayloadExit {
    use std::os::unix::process::ExitStatusExt;
    GatedDetachedPayloadExit {
        exit_code: status.code(),
        signal: status.signal(),
    }
}

/// `setsid(1)` is a Rust-side launch mechanism, so it is located on this process's `PATH`;
/// the payload's `PATH` (the manifest env) must not decide whether the launcher exists.
fn find_setsid() -> Option<PathBuf> {
    use std::os::unix::fs::PermissionsExt;

    let path = pi_js_compat::process_env::get("PATH")?;
    std::env::split_paths(&path)
        .map(|directory| directory.join("setsid"))
        .find(|candidate| {
            std::fs::metadata(candidate)
                .is_ok_and(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
        })
}

fn open_shared_output_file(stdout_path: &JsString, stderr_path: &JsString) -> Result<File, DetachedPayloadError> {
    if stdout_path != stderr_path {
        return Err(DetachedPayloadError::SplitOutput);
    }
    OpenOptions::new()
        .create_new(true)
        .append(true)
        .mode(0o600)
        .open(encode_node_utf8(stdout_path))
        .map_err(|error| node_filesystem_error(error, "open", stdout_path))
}

fn write_identity_file(path: &JsString, identity: &PayloadIdentity) -> Result<(), DetachedPayloadError> {
    let temporary_path = js_format!("{}.tmp", path);
    let mut file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .mode(0o600)
        .open(encode_node_utf8(&temporary_path))
        .map_err(|error| node_filesystem_error(error, "open", &temporary_path))?;
    file.write_all(format!("{}\n", serde_json::to_string(identity)?).as_bytes())?;
    file.sync_all()?;
    drop(file);
    std::fs::rename(encode_node_utf8(&temporary_path), encode_node_utf8(path))?;
    let directory = node_dirname(path);
    File::open(encode_node_utf8(&directory))?.sync_all()?;
    Ok(())
}

fn node_filesystem_error(error: io::Error, syscall: &str, path: &JsString) -> DetachedPayloadError {
    let message = pi_js_compat::node_fs::node_fs_error(&error, syscall, path);
    DetachedPayloadError::Io(io::Error::other(crate::core::extensions::types::HandlerError::new(
        message,
    )))
}

impl From<serde_json::Error> for DetachedPayloadError {
    fn from(source: serde_json::Error) -> Self {
        Self::Io(io::Error::other(source))
    }
}
