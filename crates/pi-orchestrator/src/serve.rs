//! Native serve lifecycle; propagated failures map to exit 1 in the CLI embedding.
use crate::{
    config::Config,
    handler::OrchestratorHandler,
    ipc::server::{IpcServer, start_ipc_server},
    radius::{RadiusOptions, RadiusPresence, radius_orchestrator_base_url},
    storage::{Storage, remove_if_exists},
    supervisor::{OrchestratorSupervisor, SupervisorOptions},
};
use pi_js_compat::{
    js_format,
    js_json::{JsString, encode_node_utf8},
};
use std::sync::Arc;

pub struct RunningOrchestrator {
    config: Config,
    supervisor: Arc<OrchestratorSupervisor>,
    radius: Arc<RadiusPresence>,
    server: tokio::sync::Mutex<Option<IpcServer>>,
    shutdown: tokio::sync::Mutex<Option<Result<(), JsString>>>,
    pub logs: Vec<JsString>,
}
impl RunningOrchestrator {
    pub async fn start_default(config: Config) -> Result<Self, pi_ai::request::CallbackError> {
        let options = RadiusOptions::from_environment().map_err(pi_ai::request::CallbackError::from)?;
        Self::start_with_radius_options(config, options)
            .await
            .map_err(Into::into)
    }
    pub(crate) async fn start_with_radius_options(config: Config, options: RadiusOptions) -> Result<Self, JsString> {
        let storage = Storage(config.dir.clone());
        let radius = RadiusPresence::new(storage.clone(), pi_ai::http::default_fetch(), options);
        let supervisor = OrchestratorSupervisor::new(storage, radius.clone(), SupervisorOptions::default());
        Self::start(config, supervisor, radius).await
    }
    pub async fn start(
        config: Config,
        supervisor: Arc<OrchestratorSupervisor>,
        radius: Arc<RadiusPresence>,
    ) -> Result<Self, JsString> {
        std::fs::create_dir_all(encode_node_utf8(&config.dir))
            .map_err(|error| pi_js_compat::node_fs::node_fs_error(&error, "mkdir", &config.dir))?;
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
    pub async fn shutdown(&self) -> Result<(), JsString> {
        let mut previous = self.shutdown.lock().await;
        if let Some(result) = previous.as_ref() {
            return result.clone();
        }
        let result = self.shutdown_once().await;
        *previous = Some(result.clone());
        result
    }
    async fn shutdown_once(&self) -> Result<(), JsString> {
        if let Some(server) = self.server.lock().await.take() {
            server.close().await;
        }
        self.supervisor.shutdown().await?;
        self.radius.stop().await?;
        remove_if_exists(&self.config.socket_path())
    }
    pub async fn run_until_signal(&self) -> Result<(), JsString> {
        use tokio::signal::unix::{SignalKind, signal};
        let mut interrupt = signal(SignalKind::interrupt()).map_err(|e| JsString::from(e.to_string()))?;
        let mut terminate = signal(SignalKind::terminate()).map_err(|e| JsString::from(e.to_string()))?;
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
async fn wait_for_fatal(mut errors: tokio::sync::watch::Receiver<Option<JsString>>) -> JsString {
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
) -> Result<Vec<JsString>, JsString> {
    supervisor.recover_after_restart().await?;
    let mut logs = vec![];
    if radius.enabled() {
        let machine = radius.start(None).await?;
        let env = std::env::vars()
            .map(|(key, value)| (key, JsString::from(value)))
            .collect();
        logs.push(js_format!(
            "radius integration enabled: {} -> {}",
            config.socket_path(),
            radius_orchestrator_base_url(&env)?
        ));
        if let Some(machine) = machine {
            logs.push(js_format!(
                "radius machine id: {}",
                machine["id"].as_string().ok_or("machine id missing")?
            ));
        }
    } else {
        logs.push("radius integration disabled: login radius in ~/.pi/agent/auth.json or set PI_RADIUS_API_KEY".into());
    }
    logs.push(js_format!("orchestrator listening on {}", config.socket_path()));
    Ok(logs)
}
pub async fn serve() -> Result<(), pi_ai::request::CallbackError> {
    let runtime = RunningOrchestrator::start_default(Config::from_environment()).await?;
    for line in &runtime.logs {
        println!("{}", encode_node_utf8(line));
    }
    runtime.run_until_signal().await.map_err(Into::into)
}
