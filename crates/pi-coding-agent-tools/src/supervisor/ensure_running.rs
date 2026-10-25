//! Frozen Supervisor startup and version coordination.
use std::collections::HashMap;
use std::path::Path;
use std::process::Stdio;
use std::sync::{Arc, LazyLock, Mutex};
use std::time::Duration;

use pi_ai::request::{BoxFuture, now_millis};
use pi_js_compat::js_json::{self, JsObject, JsString, JsValue, encode_node_utf8};
use pi_js_compat::node_error::NativeNodeError;
use pi_js_compat::{js_format, node_timers::normalize_timeout_delay};

fn startup_error(message: impl Into<JsString>) -> NativeNodeError {
    NativeNodeError::new("Error", message)
}
use serde::{Deserialize, Serialize};

use crate::core::resident_console_transport::{
    ResidentConsoleIdentity, ResidentConsoleManagedBy, ResidentConsoleService, probe_resident_console,
};
use crate::core::runtime_process::is_verified_pi_runtime_process;
use crate::utils::child_process::node_spawn_error_message;
use crate::utils::lockfile::{self, LockOptions};
use crate::utils::paths::{node_dirname, node_join, node_resolve};

pub const SUPERVISOR_AUTOSTART_ENV: &str = "PI_SUPERVISOR_AUTOSTARTED";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SupervisorLaunchInvocation {
    pub command: JsString,
    pub args: Vec<JsString>,
    pub cwd: JsString,
    #[serde(
        serialize_with = "js_json::serialize_object",
        deserialize_with = "js_json::deserialize_object"
    )]
    pub env: JsObject,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SupervisorLaunchRuntime {
    pub argv: Vec<JsString>,
    pub exec_argv: Vec<JsString>,
    pub exec_path: JsString,
    pub home_dir: JsString,
    pub is_compiled_binary: bool,
    #[serde(
        serialize_with = "js_json::serialize_object",
        deserialize_with = "js_json::deserialize_object"
    )]
    pub env: JsObject,
}
#[derive(Debug, Clone)]
pub struct EnsureSupervisorRunningInput {
    pub control_db_path: JsString,
    pub launch_invocation: Option<SupervisorLaunchInvocation>,
    pub poll_interval_ms: Option<f64>,
    pub startup_timeout_ms: Option<f64>,
}
impl EnsureSupervisorRunningInput {
    pub fn new(control_db_path: JsString) -> Self {
        Self {
            control_db_path,
            launch_invocation: None,
            poll_interval_ms: None,
            startup_timeout_ms: None,
        }
    }
}
pub type SupervisorLaunch =
    Arc<dyn Fn(SupervisorLaunchInvocation) -> BoxFuture<'static, Result<(), NativeNodeError>> + Send + Sync>;
pub type SupervisorProbe =
    Arc<dyn Fn(JsString) -> BoxFuture<'static, Result<Option<ResidentConsoleIdentity>, NativeNodeError>> + Send + Sync>;
pub type SupervisorTerminate =
    Arc<dyn Fn(ResidentConsoleIdentity) -> BoxFuture<'static, Result<(), NativeNodeError>> + Send + Sync>;
#[derive(Clone, Default)]
pub struct EnsureSupervisorRunningDependencies {
    pub launch: Option<SupervisorLaunch>,
    pub probe: Option<SupervisorProbe>,
    pub terminate: Option<SupervisorTerminate>,
}

type StartupResult = Result<ResidentConsoleIdentity, NativeNodeError>;
type Startup = tokio::sync::watch::Sender<Option<StartupResult>>;
static SUPERVISOR_STARTUPS: LazyLock<Mutex<HashMap<JsString, Arc<Startup>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

pub fn get_supervisor_start_lock_path(path: &JsString) -> JsString {
    js_format!("{}.supervisor-start.lock", path)
}
pub fn resolve_supervisor_launch_invocation(
    runtime: &SupervisorLaunchRuntime,
) -> Result<SupervisorLaunchInvocation, NativeNodeError> {
    let mut env = runtime.env.clone();
    env.insert(SUPERVISOR_AUTOSTART_ENV.into(), JsValue::String("1".into()));
    let args = if runtime.is_compiled_binary {
        vec!["supervisor".into()]
    } else {
        let entrypoint = runtime
            .argv
            .get(1)
            .filter(|entry| !entry.is_empty())
            .ok_or_else(|| startup_error("Cannot start Supervisor without the active Pi CLI entrypoint"))?;
        let mut args = runtime.exec_argv.clone();
        args.extend([entrypoint.clone(), "supervisor".into()]);
        args
    };
    Ok(SupervisorLaunchInvocation {
        command: runtime.exec_path.clone(),
        args,
        cwd: runtime.home_dir.clone(),
        env,
    })
}
pub async fn ensure_supervisor_running(
    input: &EnsureSupervisorRunningInput,
    dependencies: &EnsureSupervisorRunningDependencies,
) -> StartupResult {
    let probe = dependencies
        .probe
        .clone()
        .unwrap_or_else(|| Arc::new(|path| Box::pin(probe_supervisor_identity(path))));
    if let Some(resident) = probe(input.control_db_path.clone()).await?
        && resident.version == *crate::config::version()
    {
        return Ok(resident);
    }
    coordinate_supervisor_startup(input, dependencies, probe).await
}
async fn coordinate_supervisor_startup(
    input: &EnsureSupervisorRunningInput,
    dependencies: &EnsureSupervisorRunningDependencies,
    probe: SupervisorProbe,
) -> StartupResult {
    let key = node_resolve("", &input.control_db_path).map_err(|error| startup_error(error.message().clone()))?;
    let mut receiver = {
        let mut startups = SUPERVISOR_STARTUPS.lock().expect("supervisor startup map");
        if let Some(startup) = startups.get(&key) {
            startup.subscribe()
        } else {
            let (sender, receiver) = tokio::sync::watch::channel(None);
            let sender = Arc::new(sender);
            startups.insert(key.clone(), sender.clone());
            let input = input.clone();
            let dependencies = dependencies.clone();
            tokio::spawn(async move {
                let result = with_supervisor_start_lock(&input, &dependencies, &probe).await;
                {
                    let mut startups = SUPERVISOR_STARTUPS.lock().expect("supervisor startup map");
                    if startups.get(&key).is_some_and(|current| Arc::ptr_eq(current, &sender)) {
                        startups.remove(&key);
                    }
                }
                sender.send_replace(Some(result));
            });
            receiver
        }
    };
    loop {
        if let Some(result) = receiver.borrow_and_update().clone() {
            return result;
        }
        receiver
            .changed()
            .await
            .map_err(|_| startup_error("Supervisor startup task ended without a result"))?;
    }
}
async fn probe_supervisor_identity(
    control_db_path: JsString,
) -> Result<Option<ResidentConsoleIdentity>, NativeNodeError> {
    let socket = js_format!("{}.supervisor-console.sock", control_db_path);
    match probe_resident_console::<JsValue>(socket, ResidentConsoleService::Supervisor).await {
        Ok(snapshot) => snapshot.identity.map(Some).ok_or_else(|| {
            startup_error("Running Supervisor does not expose process identity; restart it before retrying")
        }),
        Err(error) if error.starts_with("Resident console is unavailable:") => Ok(None),
        Err(error) => Err(startup_error(error)),
    }
}
async fn with_supervisor_start_lock(
    input: &EnsureSupervisorRunningInput,
    dependencies: &EnsureSupervisorRunningDependencies,
    probe: &SupervisorProbe,
) -> StartupResult {
    let directory = node_dirname(&input.control_db_path);
    use std::os::unix::fs::DirBuilderExt;
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(encode_node_utf8(&directory))
        .map_err(|error| {
            NativeNodeError::from(pi_js_compat::node_fs::node_fs_error_record(&error, "mkdir", &directory))
        })?;
    let options = LockOptions {
        stale_ms: 5000,
        update_ms: Some(1000),
        retries: 100,
        lockfile_path: Some(encode_node_utf8(&get_supervisor_start_lock_path(&input.control_db_path)).into()),
        realpath: false,
        on_compromised: None,
    };
    let delays = supervisor_lock_retry_delays();
    let guard =
        lockfile::acquire_with_retry_delays(Path::new(&encode_node_utf8(&input.control_db_path)), &options, &delays)
            .await
            .map_err(|error| startup_error(error.message))?;
    let result = ensure_supervisor_under_lock(input, dependencies, probe).await;
    guard.release().map_err(|error| startup_error(error.message))?;
    result
}
fn supervisor_lock_retry_delays() -> Vec<Duration> {
    let mut delays = (0..100)
        .map(|attempt| {
            let random = 1.0 + crate::utils::random::random_f64();
            let millis = (random * 25.0 * 1.1f64.powi(attempt)).round().min(100.0);
            Duration::from_millis(millis as u64)
        })
        .collect::<Vec<_>>();
    delays.sort();
    delays
}
async fn ensure_supervisor_under_lock(
    input: &EnsureSupervisorRunningInput,
    dependencies: &EnsureSupervisorRunningDependencies,
    probe: &SupervisorProbe,
) -> StartupResult {
    if let Some(resident) = probe(input.control_db_path.clone()).await? {
        if resident.version == *crate::config::version() {
            return Ok(resident);
        }
        stop_incompatible_supervisor(input, dependencies, probe, resident).await?;
    }
    let invocation = input
        .launch_invocation
        .clone()
        .map(Ok)
        .unwrap_or_else(resolve_current_supervisor_launch_invocation)?;
    match &dependencies.launch {
        Some(launch) => launch(invocation).await?,
        None => launch_supervisor(invocation).await?,
    }
    wait_for_supervisor_readiness(input, probe).await
}
async fn stop_incompatible_supervisor(
    input: &EnsureSupervisorRunningInput,
    dependencies: &EnsureSupervisorRunningDependencies,
    probe: &SupervisorProbe,
    resident: ResidentConsoleIdentity,
) -> Result<(), NativeNodeError> {
    if resident.managed_by == ResidentConsoleManagedBy::External {
        return Err(startup_error(js_format!(
            "The externally managed Supervisor version {} does not match Pi version {}; restart its service manager",
            resident.version,
            crate::config::version()
        )));
    }
    match &dependencies.terminate {
        Some(terminate) => terminate(resident.clone()).await?,
        None => terminate_supervisor(&resident)?,
    }
    wait_for_supervisor_exit(input, probe, &resident.instance_id).await
}
fn resolve_current_supervisor_launch_invocation() -> Result<SupervisorLaunchInvocation, NativeNodeError> {
    let executable = std::env::current_exe().map_err(|error| startup_error(error.to_string()))?;
    let home = crate::utils::paths::home_directory()?;
    let env = pi_js_compat::process_env::vars()
        .into_iter()
        .map(|(key, value)| (key.into(), JsValue::String(value.into())))
        .collect();
    resolve_supervisor_launch_invocation(&SupervisorLaunchRuntime {
        argv: Vec::new(),
        exec_argv: Vec::new(),
        exec_path: pi_js_compat::node_fs::decode_node_path(executable.as_os_str()),
        home_dir: home,
        is_compiled_binary: true,
        env,
    })
}

/// `setsid` is the approved native detachment mechanism; late target-exec failures are a recorded gap.
async fn launch_supervisor(invocation: SupervisorLaunchInvocation) -> Result<(), NativeNodeError> {
    preflight_supervisor_target(&invocation)?;
    let mut command = tokio::process::Command::new("setsid");
    command
        .arg(encode_node_utf8(&invocation.command))
        .args(invocation.args.iter().map(encode_node_utf8))
        .current_dir(encode_node_utf8(&invocation.cwd))
        .env_clear()
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    for (key, value) in &invocation.env {
        if let Some(value) = value.as_string() {
            command.env(encode_node_utf8(key), encode_node_utf8(value));
        }
    }
    let mut child = command.spawn().map_err(|error| {
        startup_error(js_format!(
            "Could not start Supervisor: {}",
            node_spawn_error_message("spawn", &invocation.command, &error)
        ))
    })?;
    tokio::spawn(async move {
        if let Err(error) = child.wait().await {
            eprintln!("Supervisor process error: {error}");
        }
    });
    Ok(())
}
fn preflight_supervisor_target(invocation: &SupervisorLaunchInvocation) -> Result<(), NativeNodeError> {
    let paths = if invocation.command.contains('/') {
        vec![
            node_resolve(&invocation.cwd, &invocation.command)
                .map_err(|error| startup_error(error.message().clone()))?,
        ]
    } else {
        let path = invocation
            .env
            .get(&JsString::from("PATH"))
            .and_then(JsValue::as_string)
            .cloned()
            .unwrap_or_else(|| "/usr/bin:/bin".into());
        path.split(':')
            .map(|directory| {
                node_resolve(&invocation.cwd, node_join(directory, &invocation.command))
                    .map_err(|error| startup_error(error.message().clone()))
            })
            .collect::<Result<Vec<_>, _>>()?
    };
    let mut code = rustix::io::Errno::NOENT;
    if !Path::new(&encode_node_utf8(&invocation.cwd)).is_dir() {
        return Err(spawn_preflight_error(invocation, code));
    }
    for path in paths {
        let encoded = encode_node_utf8(&path);
        if let Ok(metadata) = std::fs::metadata(&encoded) {
            if metadata.is_file() && rustix::fs::access(&encoded, rustix::fs::Access::EXEC_OK).is_ok() {
                return Ok(());
            }
            code = rustix::io::Errno::ACCESS;
        }
    }
    Err(spawn_preflight_error(invocation, code))
}
fn spawn_preflight_error(invocation: &SupervisorLaunchInvocation, errno: rustix::io::Errno) -> NativeNodeError {
    let error = std::io::Error::from_raw_os_error(errno.raw_os_error());
    startup_error(js_format!(
        "Could not start Supervisor: {}",
        node_spawn_error_message("spawn", &invocation.command, &error)
    ))
}
fn terminate_supervisor(identity: &ResidentConsoleIdentity) -> Result<(), NativeNodeError> {
    let valid = identity.pid.is_finite()
        && identity.pid.fract() == 0.0
        && identity.pid > 0.0
        && identity.pid <= f64::from(i32::MAX);
    let pid = identity.pid as u32;
    if !valid || pid == std::process::id() || !is_verified_pi_runtime_process(pid) {
        return Err(startup_error(js_format!(
            "Refusing to terminate unverified Supervisor process {}",
            pi_js_compat::number::js_number_to_string(identity.pid)
        )));
    }
    let pid = rustix::process::Pid::from_raw(pid as i32).expect("validated positive pid");
    rustix::process::kill_process(pid, rustix::process::Signal::TERM).map_err(|error| {
        startup_error(js_format!(
            "Could not terminate Supervisor process {}: kill {}",
            pi_js_compat::number::js_number_to_string(identity.pid),
            crate::utils::child_process::node_error_code(&std::io::Error::from_raw_os_error(error.raw_os_error()))
        ))
    })
}
async fn wait_for_supervisor_exit(
    input: &EnsureSupervisorRunningInput,
    probe: &SupervisorProbe,
    instance_id: &JsString,
) -> Result<(), NativeNodeError> {
    let timeout = input.startup_timeout_ms.unwrap_or(10000.0);
    let deadline = now_millis() as f64 + timeout;
    while (now_millis() as f64) < deadline {
        if probe(input.control_db_path.clone())
            .await?
            .is_none_or(|resident| resident.instance_id != *instance_id)
        {
            return Ok(());
        }
        delay(input.poll_interval_ms.unwrap_or(50.0)).await;
    }
    Err(startup_error(js_format!(
        "Supervisor instance {} did not stop within {}ms",
        instance_id,
        pi_js_compat::number::js_number_to_string(timeout)
    )))
}
async fn wait_for_supervisor_readiness(input: &EnsureSupervisorRunningInput, probe: &SupervisorProbe) -> StartupResult {
    let timeout = input.startup_timeout_ms.unwrap_or(10000.0);
    let deadline = now_millis() as f64 + timeout;
    while (now_millis() as f64) < deadline {
        if let Some(resident) = probe(input.control_db_path.clone()).await?
            && resident.version == *crate::config::version()
        {
            return Ok(resident);
        }
        delay(input.poll_interval_ms.unwrap_or(50.0)).await;
    }
    Err(startup_error(js_format!(
        "Supervisor did not become ready within {}ms",
        pi_js_compat::number::js_number_to_string(timeout)
    )))
}
async fn delay(milliseconds: f64) {
    tokio::time::sleep(Duration::from_millis(
        normalize_timeout_delay(milliseconds).trunc() as u64
    ))
    .await;
}
