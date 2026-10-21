//! Native serve lifecycle; propagated failures map to exit 1 in the CLI embedding.
use crate::{
    config::Config,
    handler::OrchestratorHandler,
    ipc::server::{IpcServer, start_ipc_server},
    radius::{RadiusOptions, RadiusPresence, radius_orchestrator_base_url},
    storage::{Storage, remove_if_exists},
    supervisor::{OrchestratorSupervisor, SupervisorOptions},
};
use std::sync::Arc;

pub struct RunningOrchestrator {
    config: Config,
    supervisor: Arc<OrchestratorSupervisor>,
    radius: Arc<RadiusPresence>,
    server: tokio::sync::Mutex<Option<IpcServer>>,
    shutdown: tokio::sync::Mutex<Option<Result<(), String>>>,
    pub logs: Vec<String>,
}
impl RunningOrchestrator {
    pub async fn start_default(config: Config) -> Result<Self, String> {
        let storage = Storage(config.dir.clone());
        let radius = RadiusPresence::new(
            storage.clone(),
            pi_ai::http::default_fetch(),
            RadiusOptions::from_environment(),
        );
        let supervisor = OrchestratorSupervisor::new(storage, radius.clone(), SupervisorOptions::default());
        Self::start(config, supervisor, radius).await
    }
    pub async fn start(
        config: Config,
        supervisor: Arc<OrchestratorSupervisor>,
        radius: Arc<RadiusPresence>,
    ) -> Result<Self, String> {
        std::fs::create_dir_all(&config.dir).map_err(|e| e.to_string())?;
        let server = start_ipc_server(&config.socket_path(), Arc::new(OrchestratorHandler(supervisor.clone()))).await?;
        let logs = initialize_presence(&config, &supervisor, &radius).await;
        let logs = match logs {
            Ok(logs) => logs,
            Err(error) => {
                server.close().await;
                remove_if_exists(&config.socket_path())?;
                return Err(error);
            }
        };
        Ok(Self {
            config,
            supervisor,
            radius,
            server: tokio::sync::Mutex::new(Some(server)),
            shutdown: tokio::sync::Mutex::new(None),
            logs,
        })
    }
    pub async fn shutdown(&self) -> Result<(), String> {
        let mut previous = self.shutdown.lock().await;
        if let Some(result) = previous.as_ref() {
            return result.clone();
        }
        let result = self.shutdown_once().await;
        *previous = Some(result.clone());
        result
    }
    async fn shutdown_once(&self) -> Result<(), String> {
        if let Some(server) = self.server.lock().await.take() {
            server.close().await;
        }
        self.supervisor.shutdown().await?;
        self.radius.stop().await?;
        remove_if_exists(&self.config.socket_path())
    }
    pub async fn run_until_signal(&self) -> Result<(), String> {
        use tokio::signal::unix::{SignalKind, signal};
        let mut interrupt = signal(SignalKind::interrupt()).map_err(|e| e.to_string())?;
        let mut terminate = signal(SignalKind::terminate()).map_err(|e| e.to_string())?;
        let fatal = wait_for_fatal(self.supervisor.subscribe_fatal_errors());
        let error = tokio::select! {
            _ = interrupt.recv() => None,
            _ = terminate.recv() => None,
            error = fatal => Some(error),
        };
        self.shutdown().await?;
        match error {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }
}
async fn wait_for_fatal(mut errors: tokio::sync::watch::Receiver<Option<String>>) -> String {
    loop {
        if let Some(error) = errors.borrow_and_update().clone() {
            return error;
        }
        if errors.changed().await.is_err() {
            return "orchestrator fatal-error channel closed".into();
        }
    }
}

async fn initialize_presence(
    config: &Config,
    supervisor: &OrchestratorSupervisor,
    radius: &Arc<RadiusPresence>,
) -> Result<Vec<String>, String> {
    supervisor.recover_after_restart().await?;
    let mut logs = vec![];
    if radius.enabled() {
        let machine = radius.start(None).await?;
        let env = std::env::vars().collect();
        logs.push(format!(
            "radius integration enabled: {} -> {}",
            config.socket_path().display(),
            radius_orchestrator_base_url(&env)?
        ));
        if let Some(machine) = machine {
            logs.push(format!(
                "radius machine id: {}",
                machine["id"].as_str().unwrap_or_default()
            ));
        }
    } else {
        logs.push("radius integration disabled: login radius in ~/.pi/agent/auth.json or set PI_RADIUS_API_KEY".into());
    }
    logs.push(format!("orchestrator listening on {}", config.socket_path().display()));
    Ok(logs)
}
pub async fn serve() -> Result<(), String> {
    let runtime = RunningOrchestrator::start_default(Config::from_environment()).await?;
    for line in &runtime.logs {
        println!("{line}");
    }
    runtime.run_until_signal().await
}
