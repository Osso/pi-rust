//! Linux process identity and Pi-runtime detection (`core/runtime-process.ts`).

use std::fmt;
use std::path::Path;

use pi_js_compat::js_json::{JsString, encode_node_utf8};
use rustix::io::Errno;
use rustix::process::{Pid, test_kill_process};

const PI_RUNTIME_ENTRYPOINT_SUFFIXES: [&str; 3] = [
    "packages/coding-agent/src/cli.ts",
    "packages/coding-agent/src/bun/cli.ts",
    "packages/coding-agent/dist/cli.js",
];

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessIdentity {
    pub pid: u32,
    pub start_time_ticks: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub incarnation: Option<JsString>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessIdentityError {
    pub message: JsString,
}

impl fmt::Display for ProcessIdentityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&encode_node_utf8(&self.message))
    }
}

impl std::error::Error for ProcessIdentityError {}

fn error(message: impl Into<JsString>) -> ProcessIdentityError {
    ProcessIdentityError {
        message: message.into(),
    }
}

struct ProcessStat {
    identity: ProcessIdentity,
    state: String,
    process_group_id: u64,
}

fn stat_fields(pid: u32) -> Result<Vec<String>, ProcessIdentityError> {
    if !cfg!(target_os = "linux") {
        return Err(error("Exact process identity requires Linux /proc".to_owned()));
    }
    let path = format!("/proc/{pid}/stat");
    let stat = std::fs::read_to_string(&path)
        .map_err(|source| error(pi_js_compat::node_fs::node_fs_error(&source, "open", &path.into())))?;
    let command_end = stat
        .rfind(')')
        .ok_or_else(|| error(format!("Invalid /proc stat for process {pid}")))?;
    Ok(stat[command_end + 2..].split_whitespace().map(str::to_owned).collect())
}

fn read_process_stat(pid: u32) -> Result<ProcessStat, ProcessIdentityError> {
    let fields = stat_fields(pid)?;
    let state = fields.first().cloned().unwrap_or_default();
    let start_time_ticks = fields.get(19).and_then(|value| value.parse::<u64>().ok());
    let process_group_id = fields.get(2).and_then(|value| value.parse::<u64>().ok());
    let (Some(start_time_ticks), Some(process_group_id)) = (start_time_ticks, process_group_id) else {
        return Err(error(format!("Invalid process stat for process {pid}")));
    };
    if state.is_empty() || start_time_ticks == 0 {
        return Err(error(format!("Invalid process stat for process {pid}")));
    }
    Ok(ProcessStat {
        identity: ProcessIdentity {
            pid,
            start_time_ticks,
            incarnation: None,
        },
        state,
        process_group_id,
    })
}

pub fn read_process_identity(pid: u32) -> Result<ProcessIdentity, ProcessIdentityError> {
    read_process_stat(pid).map(|stat| stat.identity)
}

/// Process-group id from `/proc/<pid>/stat`, used by the detached payload identity.
pub fn read_process_group_id(pid: u32) -> Result<u64, ProcessIdentityError> {
    read_process_stat(pid).map(|stat| stat.process_group_id)
}

pub fn is_process_identity_alive(identity: &ProcessIdentity) -> bool {
    match read_process_stat(identity.pid) {
        Ok(stat) => {
            stat.identity.start_time_ticks == identity.start_time_ticks && stat.state != "Z" && stat.state != "X"
        }
        Err(_) => false,
    }
}

pub fn is_superseded_runtime_identity(current: &ProcessIdentity, replacement: &ProcessIdentity) -> bool {
    let same_process = current.pid == replacement.pid && current.start_time_ticks == replacement.start_time_ticks;
    let different_incarnations = match (&current.incarnation, &replacement.incarnation) {
        (Some(current), Some(replacement)) => current != replacement,
        _ => false,
    };
    same_process && different_incarnations
}

pub fn is_pi_runtime_process_alive(pid: u32) -> bool {
    if pid == std::process::id() {
        return true;
    }
    if !process_exists(pid) {
        return false;
    }
    match try_read_process_command_line(pid) {
        Some(command_line) => command_line_is_pi_runtime(&command_line),
        None => true,
    }
}

pub fn is_verified_pi_runtime_process(pid: u32) -> bool {
    if pid == std::process::id() {
        return true;
    }
    if !process_exists(pid) {
        return false;
    }
    try_read_process_command_line(pid).is_some_and(|command_line| command_line_is_pi_runtime(&command_line))
}

fn process_exists(pid: u32) -> bool {
    let Ok(pid) = i32::try_from(pid).ok().and_then(Pid::from_raw).ok_or(()) else {
        return false;
    };
    match test_kill_process(pid) {
        Ok(()) => true,
        Err(Errno::PERM) => true,
        Err(_) => false,
    }
}

/// Linux `/proc/<pid>/cmdline`; the source's macOS `ps` fallback is a documented gap.
fn try_read_process_command_line(pid: u32) -> Option<Vec<String>> {
    if !cfg!(target_os = "linux") {
        return None;
    }
    let bytes = std::fs::read(format!("/proc/{pid}/cmdline")).ok()?;
    let command_line: Vec<String> = bytes
        .split(|byte| *byte == 0)
        .filter(|argument| !argument.is_empty())
        .map(|argument| String::from_utf8_lossy(argument).into_owned())
        .collect();
    Some(command_line)
}

pub fn command_line_is_pi_runtime(command_line: &[String]) -> bool {
    let Some(executable) = command_line.first() else {
        return false;
    };
    if executable.is_empty() {
        return false;
    }
    let executable_name = Path::new(executable)
        .file_name()
        .map(|name| name.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    if executable_name == "pi" || executable_name == "pi.exe" {
        return true;
    }
    command_line[1..].iter().any(|argument| {
        let normalized = argument.replace('\\', "/");
        PI_RUNTIME_ENTRYPOINT_SUFFIXES
            .iter()
            .any(|suffix| normalized == *suffix || normalized.ends_with(&format!("/{suffix}")))
    })
}
