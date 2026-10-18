//! Gated detached payload bootstrap (`core/detached-job-bootstrap.ts`).
//!
//! Node uses `spawn(..., { detached: true })` plus a pipe on fd 3. `unsafe_code`
//! is forbidden, so the same process shape comes from `setsid(1)` and a shell
//! redirect: stdin is the gate pipe, fd 3 is a copy of it, and the payload's
//! stdin is `/dev/null` — identical descriptors, identical pid, new session.

use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};

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
    SpawnFailed(String),
}

impl std::fmt::Display for DetachedPayloadError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(source) => source.fmt(formatter),
            Self::IdentityNotPersisted => {
                formatter.write_str("Detached payload identity must be persisted before release")
            }
            Self::AlreadyReleased => formatter.write_str("Detached payload gate was already released"),
            Self::SplitOutput => formatter.write_str("Split detached payload output is not implemented"),
            Self::SpawnFailed(command) => {
                write!(formatter, "Could not start detached payload: {command}")
            }
        }
    }
}

impl std::error::Error for DetachedPayloadError {}

impl From<io::Error> for DetachedPayloadError {
    fn from(source: io::Error) -> Self {
        Self::Io(source)
    }
}

pub struct GatedDetachedPayloadInput<'a> {
    pub args: &'a [String],
    pub command: &'a str,
    pub cwd: &'a Path,
    /// `None` inherits the parent environment.
    pub env: Option<&'a [(String, String)]>,
    pub identity_path: &'a Path,
    pub stderr_path: &'a Path,
    pub stdout_path: &'a Path,
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
    identity_path: PathBuf,
    gate: Option<std::process::ChildStdin>,
    identity_persisted: bool,
    released: bool,
}

pub fn spawn_gated_detached_payload(
    input: &GatedDetachedPayloadInput<'_>,
) -> Result<GatedDetachedPayload, DetachedPayloadError> {
    let output = open_shared_output_file(input.stdout_path, input.stderr_path)?;
    if !command_is_executable(input.command) {
        return Err(DetachedPayloadError::SpawnFailed(input.command.to_owned()));
    }
    let mut command = Command::new("setsid");
    command
        .arg("/bin/sh")
        .arg("-c")
        .arg(PAYLOAD_GATE_SCRIPT)
        .arg("pi-detached-gate")
        .arg(input.command)
        .args(input.args)
        .current_dir(input.cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::from(output.try_clone()?))
        .stderr(Stdio::from(output));
    if let Some(environment) = input.env {
        command
            .env_clear()
            .envs(environment.iter().map(|(name, value)| (name, value)));
    }
    let mut child = command
        .spawn()
        .map_err(|_| DetachedPayloadError::SpawnFailed(input.command.to_owned()))?;
    let pid = child.id();
    let gate = child.stdin.take().expect("the gate pipe is always piped");
    Ok(GatedDetachedPayload {
        child,
        pid,
        identity_path: input.identity_path.to_path_buf(),
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
        let identity = read_process_identity(self.pid).map_err(|source| io::Error::other(source.message))?;
        let identity = PayloadIdentity {
            pgid: read_process_group_id(self.pid).map_err(|source| io::Error::other(source.message))?,
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

/// `execvp` search rules: a slash runs the path directly, otherwise `PATH` is scanned.
/// The shell would report the same failure as exit 125/127 after release, so the
/// launch error is detected up front to match the source's spawn rejection.
fn command_is_executable(command: &str) -> bool {
    use std::os::unix::fs::PermissionsExt;

    let is_executable = |path: &Path| {
        std::fs::metadata(path).is_ok_and(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
    };
    if command.contains('/') {
        return is_executable(Path::new(command));
    }
    let Some(path) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&path).any(|directory| is_executable(&directory.join(command)))
}

fn open_shared_output_file(stdout_path: &Path, stderr_path: &Path) -> Result<File, DetachedPayloadError> {
    if stdout_path != stderr_path {
        return Err(DetachedPayloadError::SplitOutput);
    }
    Ok(OpenOptions::new()
        .create_new(true)
        .append(true)
        .mode(0o600)
        .open(stdout_path)?)
}

fn write_identity_file(path: &Path, identity: &PayloadIdentity) -> Result<(), DetachedPayloadError> {
    let temporary_path = PathBuf::from(format!("{}.tmp", path.display()));
    let mut file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .mode(0o600)
        .open(&temporary_path)?;
    file.write_all(format!("{}\n", serde_json::to_string(identity)?).as_bytes())?;
    file.sync_all()?;
    drop(file);
    std::fs::rename(&temporary_path, path)?;
    let directory = path.parent().unwrap_or_else(|| Path::new("."));
    File::open(directory)?.sync_all()?;
    Ok(())
}

impl From<serde_json::Error> for DetachedPayloadError {
    fn from(source: serde_json::Error) -> Self {
        Self::Io(io::Error::other(source))
    }
}
