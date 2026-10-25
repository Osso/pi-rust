//! Self-restart request handoff (`core/self-restart.ts`).

use pi_ai::request::CallbackError;
use pi_js_compat::js_json::{JsObject, JsValue};
use pi_js_compat::node_error::{NativeErrorClass, NativeNodeError};
mod native_execve;

use std::collections::BTreeMap;
use std::io;
use std::process::Stdio;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use pi_agent_core::harness::messages::CustomMessageContent;
use pi_ai::request::BoxFuture;
use pi_js_compat::js_json::{JsString, encode_node_utf8};

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
    pub session_file: JsString,
    pub prompt: Option<pi_js_compat::js_json::JsString>,
    pub old_pid: Option<u32>,
}

/// `SelfRestartHandoff`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelfRestartHandoff {
    pub session_file: JsString,
    pub prompt: Option<pi_js_compat::js_json::JsString>,
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

pub type RestartEnvironment = indexmap::IndexMap<JsString, JsString>;
pub type RestartExecve =
    Arc<dyn Fn(&JsString, &[JsString], &RestartEnvironment) -> Result<(), pi_ai::request::CallbackError> + Send + Sync>;
pub type RestartExit = Arc<dyn Fn(f64) -> Result<(), pi_ai::request::CallbackError> + Send + Sync>;
pub type RestartSpawn = Arc<
    dyn Fn(&JsString, &[JsString], RestartSpawnOptions) -> Result<RestartChildProcess, pi_ai::request::CallbackError>
        + Send
        + Sync,
>;
pub type RestartSpawnSelf = Arc<
    dyn Fn(
            SelfRestartRequest,
            SelfRestartDependencies,
        ) -> BoxFuture<'static, Result<f64, pi_ai::request::CallbackError>>
        + Send
        + Sync,
>;
pub type RestartUnref = Arc<dyn Fn() -> Result<(), pi_ai::request::CallbackError> + Send + Sync>;

#[derive(Clone)]
pub struct RestartSpawnOptions {
    pub cwd: JsString,
    pub env: RestartEnvironment,
    pub stdio: &'static str,
}

pub struct RestartChildProcess {
    pub unref: Option<RestartUnref>,
    pub exit: BoxFuture<'static, Result<Option<f64>, pi_ai::request::CallbackError>>,
}

/// Frozen restart/target/spawn dependency fields, without compatibility flags.
#[derive(Clone)]
pub struct SelfRestartDependencies {
    pub argv: Option<Vec<JsString>>,
    pub argv0: Option<JsString>,
    pub exec_argv: Option<Vec<JsString>>,
    /// Default is the real POSIX call; explicit None selects frozen spawn handoff.
    pub execve: Option<RestartExecve>,
    pub exit: Option<RestartExit>,
    pub spawn: Option<RestartSpawn>,
    pub spawn_self_restart: Option<RestartSpawnSelf>,
    pub wait_for_exit: Option<bool>,
}

impl Default for SelfRestartDependencies {
    fn default() -> Self {
        Self {
            argv: None,
            argv0: None,
            exec_argv: None,
            execve: Some(Arc::new(native_execve::execve)),
            exit: None,
            spawn: None,
            spawn_self_restart: None,
            wait_for_exit: None,
        }
    }
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
    let session_file = session_file.filter(|path| !path.is_empty())?;
    let old_pid = match old_pid_raw {
        Some(raw) if !raw.is_empty() => {
            let parsed = pi_js_compat::number::parse_number(&raw);
            let is_valid_pid = is_safe_integer(parsed) && parsed > 0.0;
            let is_own_restart = parsed == f64::from(current_pid) || parsed == f64::from(parent_pid);
            if !is_valid_pid || !is_own_restart {
                return None;
            }
            Some(parsed as u32)
        }
        _ => None,
    };
    Some(SelfRestartHandoff {
        session_file: session_file.into(),
        prompt: prompt.filter(|prompt| !prompt.is_empty()).map(JsString::from),
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
    kill_probe_result_is_alive(rustix::process::test_kill_process(pid))
}

fn kill_probe_result_is_alive(result: Result<(), rustix::io::Errno>) -> bool {
    result.is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    mod oracle {
        include!(concat!(
            concat!(env!("CARGO_MANIFEST_DIR"), "/../pi-coding-agent"),
            "/tests/support/oracle.rs"
        ));
    }
    #[test]
    fn kill_result_classification_matches_the_real_frozen_default_probe() {
        let mut source = oracle::TsOracle::spawn("self-restart-state-oracle.mts", "packages/coding-agent/src");
        for (code, result) in [
            ("OK", Ok(())),
            ("EPERM", Err(rustix::io::Errno::PERM)),
            ("ESRCH", Err(rustix::io::Errno::SRCH)),
        ] {
            let expected = source.request(&serde_json::json!({"op":"probe","killError":code}));
            assert_eq!(
                serde_json::json!({"waited":kill_probe_result_is_alive(result)}),
                expected,
                "{code}"
            );
        }
    }
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
    if old_pid == 0 || old_pid == dependencies.current_pid {
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
    parsed.provider = None;
    parsed.model = None;
    parsed.thinking = None;
    parsed.resume = Some(false);
    parsed.continue_session = Some(true);
    parsed.fork = None;
    parsed.no_session = Some(false);
    parsed.session_id = None;
    parsed.file_args = Vec::new();
    parsed.messages = Vec::new();
}

/// Frozen `resolveRestartInterruptedToolCalls`: append results in pending call order.
pub fn resolve_restart_interrupted_tool_calls(
    session_manager: &mut SessionManager,
) -> Result<(), pi_ai::request::CallbackError> {
    let messages = session_manager.build_session_context().messages;
    let Some(pending) = pi_agent_core::restored_pending_tool_message(&messages) else {
        return Ok(());
    };
    let jobs = read_detached_jobs(session_manager)?;
    for content in pending.content {
        let pi_ai::types::AssistantContent::ToolCall(call) = content else {
            continue;
        };
        let job = jobs.iter().find(|job| {
            job.get("worker").and_then(|worker| worker.get("toolCallId"))
                == Some(&pi_js_compat::js_json::JsValue::String(call.id.as_str().into()))
        });
        let result = create_restart_tool_result(&call, job)?;
        session_manager
            .append_message(pi_agent_core::agent::AgentMessage::ToolResult(result))
            .map_err(pi_ai::request::CallbackError::from)?;
    }
    Ok(())
}

fn read_detached_jobs(
    session_manager: &SessionManager,
) -> Result<Vec<pi_js_compat::js_json::JsValue>, pi_ai::request::CallbackError> {
    let Some(db) = session_manager
        .get_metadata_control_db_path()
        .filter(|path| !path.is_empty())
    else {
        return Ok(Vec::new());
    };
    let Some(path) = session_manager.get_session_file().filter(|path| !path.is_empty()) else {
        return Ok(Vec::new());
    };
    let state = crate::core::session_control_db::read_multi_agent_state(db, path)
        .map_err(pi_ai::request::CallbackError::from)?;
    Ok(state
        .map(|state| state.agents)
        .unwrap_or_default()
        .into_iter()
        .filter(|job| job.get("detached").is_some_and(pi_js_compat::js_json::is_truthy))
        .collect())
}

fn create_restart_tool_result(
    call: &pi_ai::types::ToolCall,
    job: Option<&pi_js_compat::js_json::JsValue>,
) -> Result<pi_ai::types::ToolResultMessage, pi_ai::request::CallbackError> {
    use pi_js_compat::js_json::{JsObject, JsValue};
    let (text, details, is_error) = if let Some(job) = job {
        let id = match job.get("id") {
            Some(value) => pi_js_compat::js_json::to_js_string(value).map_err(pi_ai::request::CallbackError::from)?,
            None => "undefined".into(),
        };
        let mut details = JsObject::new();
        if let Some(id) = job.get("id") {
            details.insert("backgroundJobId".into(), id.clone());
        }
        details.insert("type".into(), JsValue::String("detached".into()));
        (
            pi_js_compat::js_format!(
                "Moved to background as job {} during a Pi restart; its result arrives as a job notification.",
                id
            ),
            Some(JsValue::Object(details)),
            false,
        )
    } else if call.name == "restart_self" {
        ("Pi restarted.".into(), None, false)
    } else {
        (
            "Interrupted by a Pi restart before it finished; rerun it if still needed.".into(),
            None,
            true,
        )
    };
    Ok(pi_ai::types::ToolResultMessage {
        role: pi_ai::types::ToolResultRole::Tag,
        tool_call_id: call.id.clone(),
        tool_name: call.name.clone(),
        content: vec![pi_ai::types::ToolResultContent::Text(pi_ai::types::TextContent::new(
            text,
        ))],
        details,
        is_error,
        timestamp: pi_js_compat::date::js_now_millis(),
    })
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
    let Some(prompt) = handoff.prompt.as_ref().filter(|prompt| !prompt.is_empty()) else {
        return Ok(());
    };
    let notice = format_restart_notice(prompt, handoff.old_pid, current_pid);
    session_manager.append_custom_message_entry(
        &"self_restart".into(),
        CustomMessageContent::Text(notice),
        true,
        None,
    )?;
    Ok(())
}

fn format_restart_notice(prompt: &JsString, old_pid: Option<u32>, current_pid: u32) -> JsString {
    let suffix = match old_pid {
        Some(old_pid) if old_pid != 0 && old_pid != current_pid => format!(" PID {old_pid} -> {current_pid}."),
        _ => format!(" PID: {current_pid}."),
    };
    prompt.clone() + suffix
}

/// `getSelfRestartArgs`.
pub fn get_self_restart_args(exec_argv: &[JsString], argv: &[JsString]) -> Vec<JsString> {
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

fn is_bun_virtual_entrypoint(path: &JsString) -> bool {
    BUN_VIRTUAL_ENTRYPOINT_PREFIXES
        .iter()
        .any(|prefix| path.starts_with(*prefix))
}

/// `buildSelfRestartEnv`.
pub fn build_self_restart_env(
    request: &SelfRestartRequest,
    base: &RestartEnvironment,
    current_pid: u32,
) -> RestartEnvironment {
    let mut env = base.clone();
    env.insert(ENV_SELF_RESTART_REQUEST.into(), "1".into());
    env.insert(ENV_SELF_RESTART_SESSION.into(), request.session_file.clone());
    env.insert(
        ENV_SELF_RESTART_PROMPT.into(),
        request.prompt.clone().unwrap_or_default(),
    );
    env.insert(
        ENV_SELF_RESTART_OLD_PID.into(),
        request.old_pid.unwrap_or(current_pid).to_string().into(),
    );
    // Object spread/entries puts array-index keys first and retains all other
    // insertion positions, including overwritten restart variables.
    let keys: JsObject = env.keys().map(|key| (key.clone(), JsValue::Null)).collect();
    pi_js_compat::js_json::object_entries_lossless(&keys)
        .into_iter()
        .map(|(key, _)| (key.clone(), env[key].clone()))
        .collect()
}

fn read_current_environment() -> RestartEnvironment {
    pi_js_compat::process_env::vars()
        .into_iter()
        .map(|(key, value)| (key.into(), value.into()))
        .collect()
}

fn read_process_executable() -> Result<JsString, CallbackError> {
    std::env::current_exe()
        .map(|path| pi_js_compat::node_fs::decode_node_path(path.as_os_str()))
        .map_err(restart_io_error)
}

fn read_process_argv() -> Vec<JsString> {
    std::env::args_os()
        .map(|arg| pi_js_compat::node_fs::decode_node_path(&arg))
        .collect()
}

fn read_restart_args(dependencies: &SelfRestartDependencies) -> Vec<JsString> {
    let argv = dependencies.argv.clone().unwrap_or_else(read_process_argv);
    get_self_restart_args(dependencies.exec_argv.as_deref().unwrap_or(&[]), &argv)
}

/// Frozen exec helper: no OS projection before an injected callback.
fn exec_self_restart(
    request: &SelfRestartRequest,
    execve: &RestartExecve,
    dependencies: &SelfRestartDependencies,
) -> Result<(), CallbackError> {
    let argv0 = dependencies.argv0.clone().unwrap_or_else(|| {
        pi_js_compat::node_fs::decode_node_path(&std::env::args_os().next().expect("process argv0"))
    });
    let mut args = vec![argv0];
    args.extend(read_restart_args(dependencies));
    execve(
        &read_process_executable()?,
        &args,
        &build_self_restart_env(request, &read_current_environment(), std::process::id()),
    )
}

/// `restartCurrentProcess` including injectable unexpected-return branches.
pub async fn restart_current_process(
    request: SelfRestartRequest,
    dependencies: &SelfRestartDependencies,
) -> Result<(), CallbackError> {
    let request = SelfRestartRequest {
        old_pid: Some(std::process::id()),
        ..request
    };
    if let Some(execve) = &dependencies.execve {
        exec_self_restart(&request, execve, dependencies)?;
        return Err(CallbackError::new("execve returned after replacing the process image"));
    }
    // Frozen deliberately forwards only waitForExit, not argv/argv0/execArgv.
    let spawn_dependencies = SelfRestartDependencies {
        wait_for_exit: Some(false),
        ..Default::default()
    };
    let code = match &dependencies.spawn_self_restart {
        Some(spawn) => spawn(request, spawn_dependencies).await?,
        None => spawn_self_restart(&request, &spawn_dependencies).await?,
    };
    match &dependencies.exit {
        Some(exit) => exit(code)?,
        None => exit_current_process(code)?,
    }
    Err(CallbackError::new(
        "process.exit returned after spawning restart process",
    ))
}

/// `spawnSelfRestart`: spawn/unref errors remain raw rejections; null exit is zero.
pub async fn spawn_self_restart(
    request: &SelfRestartRequest,
    dependencies: &SelfRestartDependencies,
) -> Result<f64, CallbackError> {
    let cwd =
        pi_js_compat::process_cwd::cwd().map_err(|error| CallbackError::from_error(error.message.clone(), error))?;
    let options = RestartSpawnOptions {
        cwd,
        env: build_self_restart_env(request, &read_current_environment(), std::process::id()),
        stdio: "inherit",
    };
    let executable = read_process_executable()?;
    let args = read_restart_args(dependencies);
    let child = match &dependencies.spawn {
        Some(spawn) => spawn(&executable, &args, options)?,
        None => default_spawn(&executable, &args, options)?,
    };
    if dependencies.wait_for_exit == Some(false) {
        spawn_unref_child_error_observer(child.exit);
        if let Some(unref) = child.unref {
            unref()?;
        }
        return Ok(0.0);
    }
    Ok(child.exit.await?.unwrap_or(0.0))
}

fn spawn_unref_child_error_observer(exit: BoxFuture<'static, Result<Option<f64>, CallbackError>>) {
    // Unhandled child errors stay uncaught; this observer never keeps main alive.
    drop(tokio::spawn(async move {
        if let Err(error) = exit.await {
            crate::utils::uncaught::throw_callback_uncaught(unhandled_child_error(error));
        }
    }));
}

fn exit_current_process(code: f64) -> Result<(), CallbackError> {
    let constraint = if !code.is_finite() || code.fract() != 0.0 {
        Some("an integer")
    } else if !is_safe_integer(code) {
        Some(">= -9007199254740991 && <= 9007199254740991")
    } else {
        None
    };
    if let Some(constraint) = constraint {
        let received = pi_js_compat::node_error::format_range_number(code);
        return Err(NativeNodeError::from_node_constructor(
            NativeErrorClass::RangeError,
            "ERR_OUT_OF_RANGE",
            format!("The value of \"code\" is out of range. It must be {constraint}. Received {received}"),
        )
        .into());
    }
    // Node ReallyExit uses V8 Int32Value after the exitCode setter validates.
    let unsigned = code.rem_euclid(4_294_967_296.0);
    let signed = if unsigned >= 2_147_483_648.0 {
        unsigned - 4_294_967_296.0
    } else {
        unsigned
    };
    std::process::exit(signed as i32)
}

fn unhandled_child_error(error: CallbackError) -> CallbackError {
    let pi_js_compat::abort::AbortReason::Value(value) = error.thrown_value() else {
        return error;
    };
    let inspected = pi_js_compat::node_fs::inspect_console_value(&value);
    let native = NativeNodeError::from_node_constructor(
        NativeErrorClass::Error,
        "ERR_UNHANDLED_ERROR",
        pi_js_compat::js_format!("Unhandled error. ({})", inspected),
    )
    .with_property("context", value);
    CallbackError::from(native).with_original(Arc::new(error))
}

fn default_spawn(
    file: &JsString,
    args: &[JsString],
    options: RestartSpawnOptions,
) -> Result<RestartChildProcess, CallbackError> {
    pi_js_compat::node_child_process::validate_spawn_arguments(file, args)
        .map_err(|error| CallbackError::from(*error))?;
    validate_spawn_environment(&options.env)?;
    let mut command = create_spawn_command(file, args, options);
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => return failed_spawn_child(error, file, args),
    };
    Ok(RestartChildProcess {
        // Native handles/tasks do not keep main alive; no Node ref is held.
        unref: None,
        exit: Box::pin(async move {
            let status = child.wait().await.map_err(restart_io_error)?;
            Ok(status.code().map(f64::from))
        }),
    })
}

fn validate_spawn_environment(env: &RestartEnvironment) -> Result<(), CallbackError> {
    use pi_js_compat::node_child_process::validate_string_argument;
    for (key, value) in env {
        let label = format!("options.env['{}']", encode_node_utf8(key));
        validate_string_argument(&label, "must be a string without null bytes", key)
            .map_err(|error| CallbackError::from(*error))?;
        validate_string_argument(&label, "must be a string without null bytes", value)
            .map_err(|error| CallbackError::from(*error))?;
    }
    Ok(())
}

fn create_spawn_command(file: &JsString, args: &[JsString], options: RestartSpawnOptions) -> tokio::process::Command {
    let mut command = tokio::process::Command::new(encode_node_utf8(file));
    command
        .args(args.iter().map(encode_node_utf8))
        .current_dir(encode_node_utf8(&options.cwd))
        .env_clear()
        .envs(
            options
                .env
                .into_iter()
                .map(|(key, value)| (encode_node_utf8(&key), encode_node_utf8(&value))),
        )
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    command
}

fn failed_spawn_child(
    error: io::Error,
    file: &JsString,
    args: &[JsString],
) -> Result<RestartChildProcess, CallbackError> {
    let record = pi_js_compat::node_fs::node_spawn_error_record(&error, file);
    // Node queues these launch failures as child 'error' events; other codes
    // (including E2BIG/ENOTDIR) throw synchronously from spawn.
    let deferred = matches!(
        record.code.as_str(),
        "EACCES" | "EAGAIN" | "EMFILE" | "ENFILE" | "ENOENT"
    );
    let native = NativeNodeError::from(record);
    let native = if deferred {
        native.with_property(
            "spawnargs",
            JsValue::Array(args.iter().cloned().map(JsValue::String).collect()),
        )
    } else {
        native
    };
    let error = CallbackError::from(native).with_original(Arc::new(error));
    if !deferred {
        return Err(error);
    }
    Ok(RestartChildProcess {
        unref: None,
        exit: Box::pin(async move { Err(error) }),
    })
}

fn restart_io_error(error: io::Error) -> CallbackError {
    CallbackError::from_error(JsString::from(error.to_string()), error)
}

#[cfg(test)]
mod process_tests {
    use super::*;

    // Old executable-override fixture belongs at the real backend boundary:
    // the frozen public restart API always selects process.execPath.
    #[tokio::test]
    async fn rejects_when_the_restarted_child_fails_to_spawn() {
        let error = default_spawn(
            &"/definitely/missing/pi".into(),
            &[],
            RestartSpawnOptions {
                cwd: pi_js_compat::process_cwd::cwd().unwrap(),
                env: RestartEnvironment::new(),
                stdio: "inherit",
            },
        )
        .unwrap_or_else(|error| panic!("expected deferred spawn error event, got immediate {error}"))
        .exit
        .await
        .expect_err("spawn failure");
        let original = error
            .original
            .as_ref()
            .unwrap()
            .downcast_ref::<std::io::Error>()
            .expect("spawn retains its native IO source");
        assert_eq!(original.kind(), std::io::ErrorKind::NotFound);
    }
}
