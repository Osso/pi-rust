//! Port of `packages/coding-agent/src/utils/shell.ts`.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::{Mutex, OnceLock};

use rustix::process::{Pid, Signal, kill_process_group};

use crate::config::get_bin_dir;

/// How the shell receives the command text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandTransport {
    /// The command is the last argument (`bash -c <command>`).
    Argv,
    /// The command is written to stdin (`bash -s`), used by the legacy WSL bash shim.
    Stdin,
}

/// The resolved shell invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellConfig {
    pub shell: String,
    pub args: Vec<String>,
    pub command_transport: CommandTransport,
}

/// Failure resolving the shell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellError {
    message: String,
}

impl std::fmt::Display for ShellError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for ShellError {}

/// `C:\Windows\System32\bash.exe` and the Sysnative alias are the legacy WSL bash shim, which reads
/// commands from stdin.
fn is_legacy_wsl_bash_path(path: &str) -> bool {
    let normalized = path.replace('/', "\\").to_lowercase();
    let regex = regex::Regex::new(r"^[a-z]:\\windows\\(?:system32|sysnative)\\bash\.exe$").expect("static regex");
    regex.is_match(&normalized)
}

fn get_bash_shell_config(shell: &str) -> ShellConfig {
    if is_legacy_wsl_bash_path(shell) {
        return ShellConfig {
            shell: shell.to_owned(),
            args: vec!["-s".to_owned()],
            command_transport: CommandTransport::Stdin,
        };
    }
    ShellConfig {
        shell: shell.to_owned(),
        args: vec!["-c".to_owned()],
        command_transport: CommandTransport::Argv,
    }
}

fn find_bash_on_path() -> Option<String> {
    if cfg!(windows) {
        let result = crate::utils::child_process::spawn_process_sync("where", &["bash.exe"], None).ok()?;
        if !result.status.success() {
            return None;
        }
        let stdout = String::from_utf8_lossy(&result.stdout);
        let first_match = stdout.trim().split(['\r', '\n']).next()?;
        return (!first_match.is_empty() && Path::new(first_match).exists()).then(|| first_match.to_owned());
    }

    let result = crate::utils::child_process::spawn_process_sync("which", &["bash"], None).ok()?;
    if !result.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&result.stdout);
    stdout
        .trim()
        .split(['\r', '\n'])
        .next()
        .filter(|first_match| !first_match.is_empty())
        .map(str::to_owned)
}

/// `getShellConfig`: explicit shell, then `/bin/bash`, then `bash` on `PATH`, then `sh`.
pub fn get_shell_config(custom_shell_path: Option<&str>) -> Result<ShellConfig, ShellError> {
    if let Some(custom) = custom_shell_path {
        return custom_shell_config(custom);
    }
    if cfg!(windows) {
        return windows_shell_config();
    }
    unix_shell_config()
}

fn custom_shell_config(custom: &str) -> Result<ShellConfig, ShellError> {
    if Path::new(custom).exists() {
        return Ok(get_bash_shell_config(custom));
    }
    Err(ShellError {
        message: format!("Custom shell path not found: {custom}"),
    })
}

fn windows_shell_config() -> Result<ShellConfig, ShellError> {
    let mut searched = Vec::new();
    for variable in ["ProgramFiles", "ProgramFiles(x86)"] {
        let Some(program_files) = std::env::var(variable).ok().filter(|value| !value.is_empty()) else {
            continue;
        };
        let path = format!("{program_files}\\Git\\bin\\bash.exe");
        if Path::new(&path).exists() {
            return Ok(get_bash_shell_config(&path));
        }
        searched.push(path);
    }
    if let Some(bash_on_path) = find_bash_on_path() {
        return Ok(get_bash_shell_config(&bash_on_path));
    }
    Err(missing_windows_bash_error(&searched))
}

fn missing_windows_bash_error(searched: &[String]) -> ShellError {
    let searched_paths = searched
        .iter()
        .map(|path| format!("  {path}"))
        .collect::<Vec<_>>()
        .join("\n");
    ShellError {
        message: format!(
            "No bash shell found. Options:\n  1. Install Git for Windows: https://git-scm.com/download/win\n  2. Add your bash to PATH (Cygwin, MSYS2, etc.)\n  3. Set shellPath in settings.json\n\nSearched Git Bash in:\n{searched_paths}"
        ),
    }
}

fn unix_shell_config() -> Result<ShellConfig, ShellError> {
    if Path::new("/bin/bash").exists() {
        return Ok(get_bash_shell_config("/bin/bash"));
    }
    if let Some(bash_on_path) = find_bash_on_path() {
        return Ok(get_bash_shell_config(&bash_on_path));
    }
    Ok(ShellConfig {
        shell: "sh".to_owned(),
        args: vec!["-c".to_owned()],
        command_transport: CommandTransport::Argv,
    })
}

/// `getShellEnv`: the process environment with the managed binary directory on `PATH`.
pub fn get_shell_env() -> BTreeMap<String, String> {
    let bin_dir = get_bin_dir().display().to_string();
    let mut environment: BTreeMap<String, String> = std::env::vars().collect();
    let path_key = environment
        .keys()
        .find(|key| key.eq_ignore_ascii_case("path"))
        .cloned()
        .unwrap_or_else(|| "PATH".to_owned());
    let current_path = environment.get(&path_key).cloned().unwrap_or_default();
    let separator = if cfg!(windows) { ';' } else { ':' };
    let entries: Vec<&str> = current_path
        .split(separator)
        .filter(|entry| !entry.is_empty())
        .collect();
    let has_bin_dir = entries.iter().any(|entry| *entry == bin_dir);
    let updated_path = if has_bin_dir {
        current_path
    } else {
        let mut parts = vec![bin_dir];
        parts.extend(entries.iter().map(|entry| (*entry).to_owned()));
        parts.join(&separator.to_string())
    };
    environment.insert(path_key, updated_path);
    environment
}

/// `sanitizeBinaryOutput`: drop characters that break width calculation or display.
pub fn sanitize_binary_output(input: &str) -> String {
    input
        .chars()
        .filter(|character| {
            let code = *character as u32;
            if matches!(code, 0x09 | 0x0a | 0x0d) {
                return true;
            }
            if code <= 0x1f {
                return false;
            }
            !(0xfff9..=0xfffb).contains(&code)
        })
        .collect()
}

fn tracked_detached_child_pids() -> &'static Mutex<Vec<u32>> {
    static TRACKED: OnceLock<Mutex<Vec<u32>>> = OnceLock::new();
    TRACKED.get_or_init(|| Mutex::new(Vec::new()))
}

fn tracked_pids() -> std::sync::MutexGuard<'static, Vec<u32>> {
    tracked_detached_child_pids()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Detached children are tracked so they can be killed on parent shutdown.
pub fn track_detached_child_pid(pid: u32) {
    let mut tracked = tracked_pids();
    if !tracked.contains(&pid) {
        tracked.push(pid);
    }
}

pub fn untrack_detached_child_pid(pid: u32) {
    tracked_pids().retain(|tracked| *tracked != pid);
}

pub fn kill_tracked_detached_children() {
    let pids: Vec<u32> = std::mem::take(&mut tracked_pids());
    for pid in pids {
        kill_process_tree(pid);
    }
}

/// `killProcessTree`: SIGKILL the process group, falling back to the process itself.
pub fn kill_process_tree(pid: u32) {
    let Some(raw_pid) = i32::try_from(pid).ok().filter(|raw| *raw > 0) else {
        return;
    };
    let Some(pid) = Pid::from_raw(raw_pid) else {
        return;
    };
    if kill_process_group(pid, Signal::KILL).is_err() {
        let _ = rustix::process::kill_process(pid, Signal::KILL);
    }
}

#[cfg(test)]
mod tests {
    use super::is_legacy_wsl_bash_path;

    #[test]
    fn legacy_wsl_bash_predicate_matches_only_the_shim() {
        assert!(is_legacy_wsl_bash_path("C:\\Windows\\System32\\bash.exe"));
        assert!(is_legacy_wsl_bash_path("c:/windows/sysnative/bash.exe"));
        assert!(is_legacy_wsl_bash_path("D:\\WINDOWS\\SYSTEM32\\BASH.EXE"));
        assert!(!is_legacy_wsl_bash_path("/bin/bash"));
        assert!(!is_legacy_wsl_bash_path("C:\\Program Files\\Git\\bin\\bash.exe"));
        assert!(!is_legacy_wsl_bash_path("C:\\Windows\\System32\\bash.exe.bak"));
    }
}
