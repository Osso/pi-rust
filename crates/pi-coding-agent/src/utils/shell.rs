//! Port of `packages/coding-agent/src/utils/shell.ts`.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::{Mutex, OnceLock};

use rustix::process::{Pid, Signal, kill_process_group};

use crate::config::get_bin_dir;
use pi_js_compat::js_format;
use pi_js_compat::js_json::{JsString, encode_node_utf8};
use pi_js_compat::js_regex::JsRegex;

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
    pub shell: JsString,
    pub args: Vec<JsString>,
    pub command_transport: CommandTransport,
}

/// Failure resolving the shell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellError {
    message: JsString,
}

impl std::fmt::Display for ShellError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&encode_node_utf8(&self.message))
    }
}

impl std::error::Error for ShellError {}

/// `C:\Windows\System32\bash.exe` and the Sysnative alias are the legacy WSL bash shim, which reads
/// commands from stdin.
fn is_legacy_wsl_bash_path(path: impl Into<JsString>) -> bool {
    let normalized = path.into().replace("/", "\\").to_lowercase();
    let pattern = JsRegex::new(r"^[a-z]:\\windows\\(?:system32|sysnative)\\bash\.exe$").expect("static regex");
    pattern.is_match(normalized)
}

fn get_bash_shell_config(shell: impl Into<JsString>) -> ShellConfig {
    let shell = shell.into();
    let (argument, command_transport) = if is_legacy_wsl_bash_path(&shell) {
        ("-s", CommandTransport::Stdin)
    } else {
        ("-c", CommandTransport::Argv)
    };
    ShellConfig {
        shell,
        args: vec![argument.into()],
        command_transport,
    }
}

fn find_bash_on_path() -> Option<JsString> {
    let (program, args) = if cfg!(windows) {
        ("where", vec!["bash.exe"])
    } else {
        ("which", vec!["bash"])
    };
    let arguments: Vec<_> = args.into_iter().map(JsString::from).collect();
    let result = crate::utils::child_process::spawn_sync(&program.into(), &arguments, Default::default()).ok()?;
    if result.error.is_some() || result.status != Some(0) {
        return None;
    }
    let stdout = JsString::from(
        String::from_utf8_lossy(&result.stdout.expect("successful sync capture has stdout")).into_owned(),
    );
    let first_match = stdout.trim().replace("\r", "\n").split('\n').next()?;
    if first_match.is_empty() {
        return None;
    }
    if cfg!(windows) && !Path::new(&encode_node_utf8(&first_match)).exists() {
        return None;
    }
    Some(first_match)
}

impl ShellError {
    pub fn message(&self) -> &JsString {
        &self.message
    }
}

/// `getShellConfig`: explicit shell, then `/bin/bash`, then `bash` on `PATH`, then `sh`.
pub fn get_shell_config(custom_shell_path: Option<&JsString>) -> Result<ShellConfig, ShellError> {
    if let Some(custom) = custom_shell_path.filter(|path| !path.is_empty()) {
        return custom_shell_config(custom);
    }
    if cfg!(windows) {
        return windows_shell_config();
    }
    unix_shell_config()
}

fn custom_shell_config(custom: &JsString) -> Result<ShellConfig, ShellError> {
    if Path::new(&encode_node_utf8(custom)).exists() {
        return Ok(get_bash_shell_config(custom));
    }
    Err(ShellError {
        message: js_format!("Custom shell path not found: {}", custom),
    })
}

fn windows_shell_config() -> Result<ShellConfig, ShellError> {
    let mut searched = Vec::new();
    for variable in ["ProgramFiles", "ProgramFiles(x86)"] {
        let Some(program_files) = pi_js_compat::process_env::get(variable).filter(|value| !value.is_empty()) else {
            continue;
        };
        let path = JsString::from(program_files) + "\\Git\\bin\\bash.exe";
        if Path::new(&encode_node_utf8(&path)).exists() {
            return Ok(get_bash_shell_config(&path));
        }
        searched.push(path);
    }
    if let Some(bash_on_path) = find_bash_on_path() {
        return Ok(get_bash_shell_config(&bash_on_path));
    }
    Err(missing_windows_bash_error(&searched))
}

fn missing_windows_bash_error(searched: &[JsString]) -> ShellError {
    let searched_paths = JsString::join(searched.iter().map(|path| js_format!("  {}", path)), "\n");
    ShellError {
        message: js_format!(
            "No bash shell found. Options:\n  1. Install Git for Windows: https://git-scm.com/download/win\n  2. Add your bash to PATH (Cygwin, MSYS2, etc.)\n  3. Set shellPath in settings.json\n\nSearched Git Bash in:\n{}",
            searched_paths
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
        shell: "sh".into(),
        args: vec!["-c".into()],
        command_transport: CommandTransport::Argv,
    })
}

/// `getShellEnv`: the process environment with the managed binary directory on `PATH`.
pub fn get_shell_env() -> Result<BTreeMap<JsString, JsString>, crate::config::ConfigError> {
    let bin_dir = get_bin_dir()?;
    let mut environment: BTreeMap<JsString, JsString> = pi_js_compat::process_env::vars()
        .into_iter()
        .map(|(key, value)| (key.into(), value.into()))
        .collect();
    let path_key = environment
        .keys()
        .find(|key| key.to_lowercase() == "path")
        .cloned()
        .unwrap_or_else(|| "PATH".into());
    let current_path = environment.get(&path_key).cloned().unwrap_or_default();
    let separator = if cfg!(windows) { ';' } else { ':' };
    let entries: Vec<JsString> = current_path
        .split(separator)
        .filter(|entry| !entry.is_empty())
        .collect();
    let updated_path = if entries.contains(&bin_dir) {
        current_path
    } else {
        JsString::join(std::iter::once(bin_dir).chain(entries), &separator.to_string())
    };
    environment.insert(path_key, updated_path);
    Ok(environment)
}

/// `sanitizeBinaryOutput`: drop characters that break width calculation or display.
pub fn sanitize_binary_output(input: &JsString) -> JsString {
    let units = input.units().iter().copied().filter(|unit| {
        let is_layout_control = matches!(unit, 0x09 | 0x0a | 0x0d);
        let is_display_text = *unit > 0x1f && !(0xfff9..=0xfffb).contains(unit);
        is_layout_control || is_display_text
    });
    JsString::from_utf16(units.collect())
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
    fn sync_bash_path_lookup_retains_status_and_first_match() {
        use std::os::unix::fs::PermissionsExt;
        const CHILD: &str = "PI_SHELL_SYNC_LOOKUP_CHILD";
        if std::env::var_os(CHILD).is_some() {
            let expected = std::env::var("PI_SHELL_SYNC_EXPECTED")
                .ok()
                .filter(|value| !value.is_empty());
            assert_eq!(
                super::find_bash_on_path(),
                expected.map(pi_js_compat::js_json::JsString::from)
            );
            return;
        }
        for (status, output, expected, missing) in [
            (0, " /fixture/é/bash\r\n/other/bash\n", "/fixture/é/bash", false),
            (7, "/must-not-select\n", "", false),
            (0, " \r\n", "", false),
            (0, "", "", true),
        ] {
            let root = tempfile::tempdir().unwrap();
            if !missing {
                let tool = root.path().join("which");
                std::fs::write(&tool, "#!/usr/bin/python3\nimport os,sys\nassert sys.argv[1:]==['bash']\nsys.stdout.write(os.environ['PI_SHELL_SYNC_OUTPUT'])\nsys.exit(int(os.environ['PI_SHELL_SYNC_STATUS']))\n").unwrap();
                std::fs::set_permissions(&tool, std::fs::Permissions::from_mode(0o755)).unwrap();
            }
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "utils::shell::tests::sync_bash_path_lookup_retains_status_and_first_match",
                    "--nocapture",
                ])
                .env(CHILD, "1")
                .env("PATH", root.path())
                .env("PI_SHELL_SYNC_STATUS", status.to_string())
                .env("PI_SHELL_SYNC_OUTPUT", output)
                .env("PI_SHELL_SYNC_EXPECTED", expected)
                .output()
                .unwrap();
            assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        }
    }

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
