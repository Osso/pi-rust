//! Self-restart request handoff (`core/self-restart.ts`).

use std::collections::BTreeMap;
use std::io;
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use pi_agent_core::harness::messages::CustomMessageContent;
use pi_ai::request::BoxFuture;

use crate::cli::args::Args;
use crate::core::session_manager::{SessionManager, SessionManagerError};

/// `ENV_SELF_RESTART_REQUEST`.
pub const ENV_SELF_RESTART_REQUEST: &str = "PI_SELF_RESTART_REQUEST";
/// `ENV_SELF_RESTART_SESSION`.
pub const ENV_SELF_RESTART_SESSION: &str = "PI_SELF_RESTART_SESSION";
/// `ENV_SELF_RESTART_PROMPT`.
pub const ENV_SELF_RESTART_PROMPT: &str = "PI_SELF_RESTART_PROMPT";
/// `ENV_SELF_RESTART_OLD_PID`.
pub const ENV_SELF_RESTART_OLD_PID: &str = "PI_SELF_RESTART_OLD_PID";
const SELF_RESTART_PARENT_EXIT_TIMEOUT_MS: i64 = 5_000;
const SELF_RESTART_PARENT_EXIT_POLL_MS: u64 = 25;

const BUN_VIRTUAL_ENTRYPOINT_PREFIXES: [&str; 3] = ["/$bunfs/root/", "/~BUN/root/", "/%7EBUN/root/"];

/// `SelfRestartRequest`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelfRestartRequest {
    pub session_file: String,
    pub prompt: Option<String>,
    pub old_pid: Option<u32>,
}

/// `SelfRestartHandoff`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelfRestartHandoff {
    pub session_file: String,
    pub prompt: Option<String>,
    pub old_pid: Option<u32>,
}

impl SelfRestartHandoff {
    pub fn from_request(request: &SelfRestartRequest) -> Self {
        Self {
            session_file: request.session_file.clone(),
            prompt: request.prompt.clone(),
            old_pid: request.old_pid,
        }
    }
}

/// Process restart target.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RestartTarget {
    pub argv: Vec<String>,
    pub argv0: Option<String>,
    pub exec_argv: Vec<String>,
    /// Executable override; defaults to the current executable.
    pub executable: Option<String>,
}

/// Reads and removes the self-restart request from the environment.
pub fn consume_self_restart_request(
    env: &mut BTreeMap<String, String>,
    current_pid: u32,
    parent_pid: u32,
) -> Option<SelfRestartHandoff> {
    let requested = env.get(ENV_SELF_RESTART_REQUEST).map(String::as_str) == Some("1");
    let session_file = env.remove(ENV_SELF_RESTART_SESSION);
    let prompt = env.remove(ENV_SELF_RESTART_PROMPT);
    let old_pid_raw = env.remove(ENV_SELF_RESTART_OLD_PID);
    env.remove(ENV_SELF_RESTART_REQUEST);

    if !requested {
        return None;
    }
    let session_file = session_file?;
    let old_pid = match old_pid_raw {
        Some(raw) if !raw.is_empty() => {
            let parsed = pi_js_compat::number::parse_number(&raw);
            let is_valid_pid = is_safe_integer(parsed) && parsed > 0.0;
            let old_pid = parsed as u32;
            let is_own_restart = old_pid == current_pid || old_pid == parent_pid;
            if !is_valid_pid || !is_own_restart {
                return None;
            }
            Some(old_pid)
        }
        _ => None,
    };
    Some(SelfRestartHandoff {
        session_file,
        prompt: prompt.filter(|prompt| !prompt.is_empty()),
        old_pid,
    })
}

fn is_safe_integer(value: f64) -> bool {
    const MAX_SAFE_INTEGER: f64 = 9_007_199_254_740_991.0;
    value.is_finite() && value.fract() == 0.0 && value.abs() <= MAX_SAFE_INTEGER
}

/// Wait dependencies for [`wait_for_self_restart_parent_exit`].
#[derive(Clone)]
pub struct SelfRestartWaitDependencies {
    pub current_pid: u32,
    pub is_process_alive: Arc<dyn Fn(u32) -> bool + Send + Sync>,
    pub now: Arc<dyn Fn() -> i64 + Send + Sync>,
    pub sleep: Arc<dyn Fn(u64) -> BoxFuture<'static, ()> + Send + Sync>,
}

impl Default for SelfRestartWaitDependencies {
    fn default() -> Self {
        Self {
            current_pid: std::process::id(),
            is_process_alive: Arc::new(process_is_alive),
            now: Arc::new(now_millis),
            sleep: Arc::new(|milliseconds| {
                Box::pin(tokio::time::sleep(std::time::Duration::from_millis(milliseconds)))
            }),
        }
    }
}

fn process_is_alive(pid: u32) -> bool {
    let Ok(pid) = i32::try_from(pid)
        .ok()
        .and_then(rustix::process::Pid::from_raw)
        .ok_or(())
    else {
        return false;
    };
    matches!(
        rustix::process::test_kill_process(pid),
        Ok(()) | Err(rustix::io::Errno::PERM)
    )
}

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("the system clock is after the epoch")
        .as_millis() as i64
}

/// `waitForSelfRestartParentExit`.
pub async fn wait_for_self_restart_parent_exit(
    handoff: Option<&SelfRestartHandoff>,
    dependencies: &SelfRestartWaitDependencies,
) {
    let Some(old_pid) = handoff.and_then(|handoff| handoff.old_pid) else {
        return;
    };
    if old_pid == dependencies.current_pid {
        return;
    }
    let deadline = (dependencies.now)() + SELF_RESTART_PARENT_EXIT_TIMEOUT_MS;
    while (dependencies.is_process_alive)(old_pid) && (dependencies.now)() < deadline {
        (dependencies.sleep)(SELF_RESTART_PARENT_EXIT_POLL_MS).await;
    }
}

/// `applySelfRestartRequest`.
pub fn apply_self_restart_request(parsed: &mut Args, handoff: Option<&SelfRestartHandoff>) {
    let Some(handoff) = handoff else {
        return;
    };
    parsed.session = Some(handoff.session_file.clone());
    parsed.resume = Some(false);
    parsed.continue_session = Some(true);
    parsed.fork = None;
    parsed.no_session = Some(false);
    parsed.session_id = None;
    parsed.file_args = Vec::new();
    parsed.messages = Vec::new();
}

/// `appendSelfRestartNotice`.
pub fn append_self_restart_notice(
    session_manager: &mut SessionManager,
    handoff: Option<&SelfRestartHandoff>,
    current_pid: u32,
) -> Result<(), SessionManagerError> {
    let Some(handoff) = handoff else {
        return Ok(());
    };
    let Some(prompt) = handoff.prompt.as_ref() else {
        return Ok(());
    };
    let notice = format_restart_notice(prompt, handoff.old_pid, current_pid);
    session_manager.append_custom_message_entry("self_restart", CustomMessageContent::Text(notice), true, None)?;
    Ok(())
}

fn format_restart_notice(prompt: &str, old_pid: Option<u32>, current_pid: u32) -> String {
    match old_pid {
        Some(old_pid) if old_pid != current_pid => {
            format!("{prompt} PID {old_pid} -> {current_pid}.")
        }
        _ => format!("{prompt} PID: {current_pid}."),
    }
}

/// `getSelfRestartArgs`.
pub fn get_self_restart_args(exec_argv: &[String], argv: &[String]) -> Vec<String> {
    let first_real_arg_index = argv
        .iter()
        .enumerate()
        .find(|(index, argument)| *index > 0 && !is_bun_virtual_entrypoint(argument))
        .map(|(index, _)| index);
    let script_args = first_real_arg_index.map_or_else(Vec::new, |index| argv[index..].to_vec());
    let mut args = exec_argv.to_vec();
    args.extend(script_args);
    args
}

fn is_bun_virtual_entrypoint(path: &str) -> bool {
    BUN_VIRTUAL_ENTRYPOINT_PREFIXES
        .iter()
        .any(|prefix| path.starts_with(prefix))
}

/// `buildSelfRestartEnv`.
pub fn build_self_restart_env(
    request: &SelfRestartRequest,
    base: &BTreeMap<String, String>,
    current_pid: u32,
) -> BTreeMap<String, String> {
    let mut env = base.clone();
    env.insert(ENV_SELF_RESTART_REQUEST.to_owned(), "1".to_owned());
    env.insert(ENV_SELF_RESTART_SESSION.to_owned(), request.session_file.clone());
    env.insert(
        ENV_SELF_RESTART_PROMPT.to_owned(),
        request.prompt.clone().unwrap_or_default(),
    );
    env.insert(
        ENV_SELF_RESTART_OLD_PID.to_owned(),
        request.old_pid.unwrap_or(current_pid).to_string(),
    );
    env
}

fn current_environment() -> BTreeMap<String, String> {
    pi_js_compat::process_env::vars().into_iter().collect()
}

/// `execSelfRestart`: replaces the current process image via `execve`.
pub fn exec_self_restart(request: &SelfRestartRequest, target: &RestartTarget) -> io::Error {
    let executable = match &target.executable {
        Some(executable) => executable.clone(),
        None => std::env::current_exe()
            .expect("current executable")
            .to_string_lossy()
            .into_owned(),
    };
    let args = get_self_restart_args(&target.exec_argv, &target.argv);
    let env = build_self_restart_env(request, &current_environment(), std::process::id());
    let mut command = Command::new(executable);
    if let Some(argv0) = &target.argv0 {
        command.arg0(argv0);
    }
    command.args(args).env_clear().envs(env);
    command.exec()
}

/// `restartCurrentProcess`.
pub fn restart_current_process(request: SelfRestartRequest, target: &RestartTarget) -> io::Error {
    let restart_request = SelfRestartRequest {
        old_pid: Some(std::process::id()),
        ..request
    };
    exec_self_restart(&restart_request, target)
}

/// `spawnSelfRestart`.
pub fn spawn_self_restart(
    request: &SelfRestartRequest,
    target: &RestartTarget,
    wait_for_exit: bool,
) -> io::Result<i32> {
    let executable = match &target.executable {
        Some(executable) => executable.clone(),
        None => std::env::current_exe()?.to_string_lossy().into_owned(),
    };
    let env = build_self_restart_env(request, &current_environment(), std::process::id());
    let mut command = Command::new(executable);
    command
        .args(get_self_restart_args(&target.exec_argv, &target.argv))
        .current_dir(std::env::current_dir()?)
        .envs(env)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    let mut child = command.spawn()?;
    if !wait_for_exit {
        return Ok(0);
    }
    let status = child.wait()?;
    Ok(status.code().unwrap_or(0))
}
