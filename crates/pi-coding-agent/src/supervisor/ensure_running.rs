//! Frozen `supervisor/ensure-running.ts`; process effects are injectable.
use crate::core::resident_console_transport::{
    ResidentConsoleIdentity, ResidentConsoleService, ResidentManagedBy, probe_resident_console,
};
use crate::utils::lockfile::{self, LockErrorCode, LockOptions};
use futures_util::future::{FutureExt, Shared};
use pi_ai::request::BoxFuture;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

pub const SUPERVISOR_AUTOSTART_ENV: &str = "PI_SUPERVISOR_AUTOSTARTED";
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct SupervisorLaunchInvocation {
    pub command: String,
    pub args: Vec<String>,
    pub cwd: String,
    pub env: BTreeMap<String, String>,
}
pub struct SupervisorLaunchRuntime {
    pub argv: Vec<String>,
    pub exec_argv: Vec<String>,
    pub exec_path: String,
    pub home_dir: String,
    pub is_compiled_binary: bool,
    pub env: BTreeMap<String, String>,
}
#[derive(Debug, Clone)]
pub struct EnsureSupervisorRunningInput {
    pub control_db_path: String,
    pub launch_invocation: Option<SupervisorLaunchInvocation>,
    pub poll_interval_ms: Option<u64>,
    pub startup_timeout_ms: Option<u64>,
}
impl EnsureSupervisorRunningInput {
    pub fn new(control_db_path: String) -> Self {
        Self {
            control_db_path,
            launch_invocation: None,
            poll_interval_ms: None,
            startup_timeout_ms: None,
        }
    }
}
type Probe = Arc<dyn Fn(String) -> BoxFuture<'static, Result<Option<ResidentConsoleIdentity>, String>> + Send + Sync>;
type Launch = Arc<dyn Fn(SupervisorLaunchInvocation) -> BoxFuture<'static, Result<(), String>> + Send + Sync>;
type Terminate = Arc<dyn Fn(ResidentConsoleIdentity) -> BoxFuture<'static, Result<(), String>> + Send + Sync>;
#[derive(Clone)]
pub struct EnsureSupervisorRunningDependencies {
    pub probe: Probe,
    pub launch: Launch,
    pub terminate: Terminate,
}
impl Default for EnsureSupervisorRunningDependencies {
    fn default() -> Self {
        Self {
            probe: Arc::new(|path| Box::pin(probe_supervisor_identity(path))),
            launch: Arc::new(|invocation| Box::pin(launch_supervisor(invocation))),
            terminate: Arc::new(|identity| Box::pin(terminate_supervisor(identity))),
        }
    }
}
type Startup = Shared<BoxFuture<'static, Result<ResidentConsoleIdentity, String>>>;
static STARTUPS: OnceLock<Mutex<BTreeMap<PathBuf, Startup>>> = OnceLock::new();
pub fn get_supervisor_start_lock_path(path: &str) -> String {
    format!("{path}.supervisor-start.lock")
}
pub fn resolve_supervisor_launch_invocation(
    runtime: &SupervisorLaunchRuntime,
) -> Result<SupervisorLaunchInvocation, String> {
    let mut env = runtime.env.clone();
    env.insert(SUPERVISOR_AUTOSTART_ENV.into(), "1".into());
    let args = if runtime.is_compiled_binary {
        vec!["supervisor".into()]
    } else {
        let entrypoint = runtime
            .argv
            .get(1)
            .filter(|value| !value.is_empty())
            .ok_or("Cannot start Supervisor without the active Pi CLI entrypoint")?;
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
    deps: &EnsureSupervisorRunningDependencies,
) -> Result<ResidentConsoleIdentity, String> {
    if let Some(identity) = (deps.probe)(input.control_db_path.clone()).await?
        && identity.version == crate::config::version()
    {
        return Ok(identity);
    }
    let key = normalize_startup_path(&input.control_db_path)?;
    let startup = {
        let mut startups = STARTUPS.get_or_init(Default::default).lock().unwrap();
        startups
            .entry(key.clone())
            .or_insert_with(|| {
                let input = input.clone();
                let deps = deps.clone();
                async move { with_start_lock(input, deps).await }.boxed().shared()
            })
            .clone()
    };
    let result = startup.clone().await;
    let mut startups = STARTUPS.get().unwrap().lock().unwrap();
    if startups.get(&key).is_some_and(|current| current.ptr_eq(&startup)) {
        startups.remove(&key);
    }
    result
}
fn normalize_startup_path(path: &str) -> Result<PathBuf, String> {
    let absolute = std::path::absolute(path).map_err(|error| error.to_string())?;
    let mut result = PathBuf::new();
    for component in absolute.components() {
        match component {
            std::path::Component::ParentDir => {
                result.pop();
            }
            std::path::Component::CurDir => {}
            _ => result.push(component),
        }
    }
    Ok(result)
}
async fn with_start_lock(
    input: EnsureSupervisorRunningInput,
    deps: EnsureSupervisorRunningDependencies,
) -> Result<ResidentConsoleIdentity, String> {
    crate::utils::private_paths::create_private_directory(
        Path::new(&input.control_db_path).parent().unwrap_or(Path::new(".")),
    )?;
    let options = LockOptions {
        stale_ms: 5000,
        update_ms: Some(1000),
        lockfile_path: Some(get_supervisor_start_lock_path(&input.control_db_path).into()),
        realpath: false,
        ..Default::default()
    };
    let mut attempt = 0;
    let guard = loop {
        match lockfile::acquire(Path::new(&input.control_db_path), &options).await {
            Ok(guard) => break guard,
            Err(error) if error.code == LockErrorCode::Locked && attempt < 100 => {
                let random = 1.0 + crate::utils::random::random_f64();
                let millis = (25.0 * 1.1_f64.powi(attempt) * random).round().min(100.0) as u64;
                tokio::time::sleep(Duration::from_millis(millis)).await;
                attempt += 1;
            }
            Err(error) => return Err(error.to_string()),
        }
    };
    let result = ensure_under_lock(&input, &deps).await;
    guard.release().map_err(|error| error.to_string())?;
    result
}
async fn ensure_under_lock(
    input: &EnsureSupervisorRunningInput,
    deps: &EnsureSupervisorRunningDependencies,
) -> Result<ResidentConsoleIdentity, String> {
    if let Some(resident) = (deps.probe)(input.control_db_path.clone()).await? {
        if resident.version == crate::config::version() {
            return Ok(resident);
        }
        if resident.managed_by == ResidentManagedBy::External {
            return Err(format!(
                "The externally managed Supervisor version {} does not match Pi version {}; restart its service manager",
                resident.version,
                crate::config::version()
            ));
        }
        (deps.terminate)(resident.clone()).await?;
        wait_for_exit(input, deps, &resident.instance_id).await?;
    }
    let invocation = match &input.launch_invocation {
        Some(invocation) => invocation.clone(),
        None => resolve_current_launch_invocation()?,
    };
    (deps.launch)(invocation).await?;
    wait_for_readiness(input, deps).await
}
async fn wait_for_exit(
    input: &EnsureSupervisorRunningInput,
    deps: &EnsureSupervisorRunningDependencies,
    instance: &str,
) -> Result<(), String> {
    let timeout = input.startup_timeout_ms.unwrap_or(10000);
    let deadline = tokio::time::Instant::now() + Duration::from_millis(timeout);
    while tokio::time::Instant::now() < deadline {
        let resident = (deps.probe)(input.control_db_path.clone()).await?;
        if resident.is_none_or(|resident| resident.instance_id != instance) {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(input.poll_interval_ms.unwrap_or(50))).await;
    }
    Err(format!(
        "Supervisor instance {instance} did not stop within {timeout}ms"
    ))
}
async fn wait_for_readiness(
    input: &EnsureSupervisorRunningInput,
    deps: &EnsureSupervisorRunningDependencies,
) -> Result<ResidentConsoleIdentity, String> {
    let timeout = input.startup_timeout_ms.unwrap_or(10000);
    let deadline = tokio::time::Instant::now() + Duration::from_millis(timeout);
    while tokio::time::Instant::now() < deadline {
        if let Some(resident) = (deps.probe)(input.control_db_path.clone()).await?
            && resident.version == crate::config::version()
        {
            return Ok(resident);
        }
        tokio::time::sleep(Duration::from_millis(input.poll_interval_ms.unwrap_or(50))).await;
    }
    Err(format!("Supervisor did not become ready within {timeout}ms"))
}
async fn probe_supervisor_identity(path: String) -> Result<Option<ResidentConsoleIdentity>, String> {
    match probe_resident_console::<serde_json::Value>(
        &format!("{path}.supervisor-console.sock"),
        ResidentConsoleService::Supervisor,
    )
    .await
    {
        Ok(snapshot) => snapshot
            .identity
            .map(Some)
            .ok_or("Running Supervisor does not expose process identity; restart it before retrying".into()),
        Err(error) if error.starts_with("Resident console is unavailable:") => Ok(None),
        Err(error) => Err(error),
    }
}
fn resolve_current_launch_invocation() -> Result<SupervisorLaunchInvocation, String> {
    let exec_path = std::env::current_exe()
        .map_err(|error| error.to_string())?
        .display()
        .to_string();
    let home = pi_js_compat::process_env::get("HOME").unwrap_or_default();
    resolve_supervisor_launch_invocation(&SupervisorLaunchRuntime {
        argv: std::env::args().collect(),
        exec_argv: Vec::new(),
        exec_path,
        home_dir: home,
        is_compiled_binary: true,
        env: pi_js_compat::process_env::vars().into_iter().collect(),
    })
}
async fn launch_supervisor(invocation: SupervisorLaunchInvocation) -> Result<(), String> {
    let mut command = std::process::Command::new("setsid");
    command
        .arg("--")
        .arg(&invocation.command)
        .args(invocation.args)
        .current_dir(invocation.cwd)
        .env_clear()
        .envs(invocation.env)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    let mut child = command
        .spawn()
        .map_err(|error| format!("Could not start Supervisor: {error}"))?;
    std::thread::spawn(move || {
        if let Err(error) = child.wait() {
            eprintln!("Supervisor process error: {error}");
        }
    });
    Ok(())
}
async fn terminate_supervisor(identity: ResidentConsoleIdentity) -> Result<(), String> {
    let pid = identity
        .pid
        .as_u64()
        .and_then(|pid| u32::try_from(pid).ok())
        .ok_or_else(|| format!("Refusing to terminate unverified Supervisor process {}", identity.pid))?;
    if pid == std::process::id() || !crate::core::runtime_process::is_verified_pi_runtime_process(pid) {
        return Err(format!("Refusing to terminate unverified Supervisor process {pid}"));
    }
    let pid = rustix::process::Pid::from_raw(pid as i32)
        .ok_or_else(|| format!("Could not terminate Supervisor process {pid}: invalid PID"))?;
    rustix::process::kill_process(pid, rustix::process::Signal::TERM)
        .map_err(|error| format!("Could not terminate Supervisor process {}: {error}", identity.pid))
}
