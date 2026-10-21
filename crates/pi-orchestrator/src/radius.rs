//! Radius presence, with injected Fetch/clock/randomness for isolated conformance.
use crate::{
    config::{Config, VERSION},
    storage::Storage,
};
use indexmap::IndexMap;
use pi_ai::request::{BoxFuture, Fetch, HttpRequest, read_body_text};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex, Weak},
    time::Duration,
};
use tokio::task::AbortHandle;

fn nonempty<'a>(env: &'a BTreeMap<String, String>, key: &str) -> Option<&'a str> {
    env.get(key).map(String::as_str).filter(|s| !s.is_empty())
}
pub fn radius_url(env: &BTreeMap<String, String>) -> String {
    nonempty(env, "PI_RADIUS_URL")
        .unwrap_or("https://radius.pi.dev/")
        .into()
}
pub fn radius_orchestrator_base_url(env: &BTreeMap<String, String>) -> Result<String, String> {
    if let Some(url) = nonempty(env, "PI_RADIUS_ORCHESTRATOR_URL") {
        return Ok(url.into());
    }
    url::Url::parse(&radius_url(env))
        .and_then(|url| url.join("/v1/"))
        .map(|v| v.to_string())
        .map_err(|e| e.to_string())
}
fn oauth(credential: Option<&Value>) -> Option<&Value> {
    credential.filter(|v| v["type"] == "oauth")
}
pub fn radius_access_token(credential: Option<&Value>, env: &BTreeMap<String, String>) -> Result<String, String> {
    if let Some(access) = oauth(credential)
        .and_then(|c| c["access"].as_str())
        .filter(|s| !s.is_empty())
    {
        return Ok(access.into());
    }
    nonempty(env, "PI_RADIUS_API_KEY")
        .map(str::to_owned)
        .ok_or_else(|| "Radius credentials are required in ~/.pi/agent/auth.json or PI_RADIUS_API_KEY".into())
}
pub fn radius_enabled(credential: Option<&Value>, env: &BTreeMap<String, String>) -> bool {
    let access = oauth(credential).map(|v| pi_js_compat::js_json::JsValue::from(&v["access"]));
    access.as_ref().is_some_and(pi_js_compat::js_json::is_truthy) || nonempty(env, "PI_RADIUS_API_KEY").is_some()
}
pub fn compute_backoff_delay_ms(failure: u32, random: f64) -> u64 {
    let exponent = failure.saturating_sub(1).min(30);
    let exponential = (1000u64.saturating_mul(1u64 << exponent)).min(30000);
    let jitter = (random * (exponential as f64 / 4.0).max(250.0)).floor() as u64;
    (exponential + jitter).min(30000)
}
pub type Environment = Arc<dyn Fn() -> BTreeMap<String, String> + Send + Sync>;
pub struct RadiusOptions {
    pub env: Environment,
    pub credential: Arc<dyn Fn() -> Option<Value> + Send + Sync>,
    pub now: Arc<dyn Fn() -> String + Send + Sync>,
    pub random: Arc<dyn Fn() -> f64 + Send + Sync>,
    pub hostname: String,
    pub platform: String,
    pub arch: String,
    pub pid: u32,
}
impl RadiusOptions {
    pub fn from_environment() -> Self {
        let auth = Arc::new(pi_coding_agent::core::auth_storage::AuthStorage::create(None));
        Self {
            env: Arc::new(|| std::env::vars().collect()),
            credential: Arc::new(move || {
                auth.reload();
                auth.get("radius").map(|v| v.into_serde_lossy())
            }),
            now: Arc::new(|| pi_js_compat::date::to_iso_string(pi_js_compat::date::js_now_millis() as i64)),
            random: Arc::new(|| {
                let mut bytes = [0; 8];
                getrandom::fill(&mut bytes).expect("OS random source unavailable");
                (u64::from_le_bytes(bytes) >> 11) as f64 / (1u64 << 53) as f64
            }),
            hostname: gethostname::gethostname().to_string_lossy().into_owned(),
            platform: std::env::consts::OS.into(),
            arch: match std::env::consts::ARCH {
                "x86_64" => "x64",
                "aarch64" => "arm64",
                v => v,
            }
            .into(),
            pid: std::process::id(),
        }
    }
}
pub trait RadiusPresenceCoordinator: Send + Sync {
    fn get_live_instance(&self, id: &str) -> Option<Value>;
    fn list_live_instances(&self) -> Vec<Value>;
    fn update_instance(&self, instance: Value) -> Result<(), String>;
}
#[derive(Default)]
struct Heartbeat {
    timer: Option<AbortHandle>,
    interval: u64,
    radius_id: String,
    not_found: u32,
    transient: u32,
}
#[derive(Default)]
struct State {
    machine: Option<Value>,
    machine_heartbeat: Heartbeat,
    pis: IndexMap<String, Heartbeat>,
    coordinator: Option<Weak<dyn RadiusPresenceCoordinator>>,
}
pub struct RadiusPresence {
    storage: Storage,
    fetch: Arc<dyn Fetch>,
    options: RadiusOptions,
    state: Mutex<State>,
}
#[derive(Debug)]
struct RadiusError {
    status: Option<u16>,
    message: String,
}
impl std::fmt::Display for RadiusError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}
impl RadiusError {
    fn not_found(&self) -> bool {
        self.status == Some(404)
    }
}
impl From<String> for RadiusError {
    fn from(message: String) -> Self {
        Self { status: None, message }
    }
}
impl RadiusPresence {
    pub fn new(storage: Storage, fetch: Arc<dyn Fetch>, options: RadiusOptions) -> Arc<Self> {
        Arc::new(Self {
            storage,
            fetch,
            options,
            state: Mutex::new(State::default()),
        })
    }
    pub fn set_coordinator(&self, coordinator: Weak<dyn RadiusPresenceCoordinator>) {
        self.state.lock().unwrap().coordinator = Some(coordinator);
    }
    pub fn enabled(&self) -> bool {
        radius_enabled((self.options.credential)().as_ref(), &(self.options.env)())
    }
    fn coordinator(&self) -> Option<Arc<dyn RadiusPresenceCoordinator>> {
        self.state.lock().unwrap().coordinator.as_ref().and_then(Weak::upgrade)
    }
    async fn post(&self, path: &str, body: Value, read_json: bool) -> Result<Value, RadiusError> {
        let env = (self.options.env)();
        let base = radius_orchestrator_base_url(&env)?;
        let url = url::Url::parse(&base)
            .and_then(|url| url.join(path))
            .map_err(|e| e.to_string())?;
        let token = radius_access_token((self.options.credential)().as_ref(), &env)?;
        let request = HttpRequest {
            method: "POST".into(),
            url: url.to_string(),
            headers: vec![
                ("Authorization".into(), format!("Bearer {token}")),
                ("Content-Type".into(), "application/json".into()),
            ],
            body: Some(pi_js_compat::js_json::stringify(&body).into_bytes()),
        };
        let response = self.fetch.fetch(request, None).await.map_err(|e| e.to_string())?;
        let status = response.status;
        if (200..300).contains(&status) && !read_json {
            return Ok(Value::Null);
        }
        let text = read_body_text(response.body).await.map_err(|e| e.to_string())?;
        if !(200..300).contains(&status) {
            return Err(RadiusError {
                status: Some(status),
                message: format!("Radius request failed: {status} {text}"),
            });
        }
        if read_json {
            pi_js_compat::js_json::parse(&text).map_err(|e| e.to_string().into())
        } else {
            Ok(Value::Null)
        }
    }
    pub async fn start(self: &Arc<Self>, label: Option<String>) -> Result<Option<Value>, String> {
        if !self.enabled() {
            return Ok(None);
        }
        let registered = self.register_machine(label).await.map_err(|e| e.to_string())?;
        self.state.lock().unwrap().machine_heartbeat.interval =
            registered["heartbeatIntervalMs"].as_u64().unwrap_or_default();
        self.schedule_machine(registered["heartbeatIntervalMs"].as_u64().unwrap_or_default());
        Ok(self.state.lock().unwrap().machine.clone())
    }
    pub async fn stop(&self) -> Result<(), String> {
        let machine = {
            let mut state = self.state.lock().unwrap();
            abort_timer(&mut state.machine_heartbeat);
            for (_, mut heartbeat) in state.pis.drain(..) {
                abort_timer(&mut heartbeat);
            }
            state.machine.clone()
        };
        if let Some(machine) = machine.filter(|_| self.enabled()) {
            self.disconnect(&format!(
                "machines/{}/disconnect",
                machine["id"].as_str().unwrap_or_default()
            ))
            .await?;
        }
        Ok(())
    }
    async fn disconnect(&self, path: &str) -> Result<(), String> {
        match self.post(path, json!({}), false).await {
            Ok(_) => Ok(()),
            Err(e) if e.not_found() => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }
    pub async fn register_pi(self: &Arc<Self>, mut instance: Value) -> Result<Value, String> {
        if !self.enabled() {
            return Ok(instance);
        }
        let machine = self
            .load_machine_identity()?
            .ok_or("No registered machine available for Pi registration")?;
        let mut body = json!({"machineId":machine["id"],"cwd":instance["cwd"],"hostname":self.options.hostname,"pid":self.options.pid,"transport":"local-rpc","capabilities":{"rpc":true,"relay":false,"iroh":false}});
        copy_optional(&instance, &mut body, &["label", "sessionId"]);
        let registered = self.post("pis/register", body, true).await.map_err(|e| e.to_string())?;
        instance["radiusPiId"] = registered["id"].clone();
        let id = instance["id"].as_str().ok_or("instance ID missing")?.to_owned();
        let heartbeat = Heartbeat {
            interval: registered["heartbeatIntervalMs"].as_u64().unwrap_or_default(),
            radius_id: registered["id"].as_str().unwrap_or_default().into(),
            ..Default::default()
        };
        let interval = heartbeat.interval;
        if let Some(mut old) = self.state.lock().unwrap().pis.insert(id.clone(), heartbeat) {
            abort_timer(&mut old);
        }
        self.schedule_pi(&id, interval);
        Ok(instance)
    }
    pub async fn disconnect_pi(&self, instance: &Value) -> Result<(), String> {
        if let Some(mut heartbeat) = self
            .state
            .lock()
            .unwrap()
            .pis
            .shift_remove(instance["id"].as_str().unwrap_or_default())
        {
            abort_timer(&mut heartbeat);
        }
        let Some(id) = instance["radiusPiId"].as_str().filter(|v| !v.is_empty()) else {
            return Ok(());
        };
        if !self.enabled() {
            return Ok(());
        }
        self.disconnect(&format!("pis/{id}/disconnect")).await
    }
    fn load_machine_identity(&self) -> Result<Option<Value>, String> {
        let cached = self.state.lock().unwrap().machine.clone();
        match cached {
            Some(machine) => Ok(Some(machine)),
            None => self.storage.load_machine(),
        }
    }
    async fn register_machine(&self, label: Option<String>) -> Result<Value, RadiusError> {
        let existing = self.load_machine_identity()?;
        let mut body = json!({"hostname":self.options.hostname,"platform":self.options.platform,"arch":self.options.arch,"version":VERSION,"capabilities":{"spawn":true,"relay":false,"iroh":false}});
        if let Some(machine) = &existing {
            body["machineId"] = machine["id"].clone();
        }
        if let Some(label) = &label {
            body["label"] = json!(label);
        }
        let registered = self.post("machines/register", body, true).await?;
        let timestamp = (self.options.now)();
        let created = existing
            .as_ref()
            .and_then(|v| v.get("createdAt"))
            .filter(|v| !v.is_null())
            .cloned()
            .unwrap_or(json!(timestamp));
        let mut machine = json!({"id":registered["id"],"createdAt":created,"lastSeenAt":timestamp});
        if let Some(label) = label {
            machine["label"] = json!(label);
        }
        self.storage.save_machine(&machine)?;
        let mut state = self.state.lock().unwrap();
        state.machine = Some(machine);
        state.machine_heartbeat.not_found = 0;
        state.machine_heartbeat.transient = 0;
        Ok(registered)
    }
    fn schedule_machine(self: &Arc<Self>, delay: u64) {
        let this = self.clone();
        let task = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(delay.max(1))).await;
            this.state.lock().unwrap().machine_heartbeat.timer = None;
            this.heartbeat_machine().await;
        });
        let mut state = self.state.lock().unwrap();
        abort_timer(&mut state.machine_heartbeat);
        state.machine_heartbeat.timer = Some(task.abort_handle());
    }
    fn schedule_pi(self: &Arc<Self>, id: &str, delay: u64) {
        let mut state = self.state.lock().unwrap();
        let Some(heartbeat) = state.pis.get_mut(id) else {
            return;
        };
        let this = self.clone();
        let id = id.to_owned();
        let task = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(delay.max(1))).await;
            {
                if let Some(heartbeat) = this.state.lock().unwrap().pis.get_mut(&id) {
                    heartbeat.timer = None;
                }
            }
            this.heartbeat_pi(&id).await;
        });
        abort_timer(heartbeat);
        heartbeat.timer = Some(task.abort_handle());
    }
    fn heartbeat_machine(self: &Arc<Self>) -> BoxFuture<'_, ()> {
        Box::pin(async move {
            let machine = self.state.lock().unwrap().machine.clone();
            let Some(machine) = machine.filter(|_| self.enabled()) else {
                return;
            };
            let config = Config {
                dir: self.storage.0.clone(),
            };
            let result = self
                .post(
                    &format!("machines/{}/heartbeat", machine["id"].as_str().unwrap_or_default()),
                    json!({"cwd":self.storage.0,"socketPath":config.socket_path()}),
                    false,
                )
                .await;
            match result {
                Ok(_) => {
                    let interval = {
                        let mut state = self.state.lock().unwrap();
                        state.machine_heartbeat.not_found = 0;
                        state.machine_heartbeat.transient = 0;
                        state.machine_heartbeat.interval
                    };
                    self.schedule_machine(interval);
                }
                Err(error) => self.retry_machine(error).await,
            }
        })
    }
    async fn retry_machine(self: &Arc<Self>, error: RadiusError) {
        let (recover, delay, count) = {
            let mut state = self.state.lock().unwrap();
            retry_decision(&mut state.machine_heartbeat, error.not_found(), (self.options.random)())
        };
        if !recover {
            if !error.not_found() {
                log_retry("Radius machine", "heartbeat", delay, count, &error);
            }
            self.schedule_machine(delay);
            return;
        }
        if let Err(error) = self.re_register_machine_and_pis().await {
            let (delay, count) = {
                let mut state = self.state.lock().unwrap();
                state.machine_heartbeat.transient += 1;
                let count = state.machine_heartbeat.transient;
                (compute_backoff_delay_ms(count, (self.options.random)()), count)
            };
            log_retry("Radius machine", "re-registration", delay, count, &error);
            self.schedule_machine(delay);
        }
    }
    fn heartbeat_pi<'a>(self: &'a Arc<Self>, id: &'a str) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            if !self.enabled() {
                return;
            }
            let radius_id = {
                let state = self.state.lock().unwrap();
                let Some(heartbeat) = state.pis.get(id) else {
                    return;
                };
                heartbeat.radius_id.clone()
            };
            let result = self.post(&format!("pis/{radius_id}/heartbeat"), json!({}), false).await;
            match result {
                Ok(_) => {
                    let interval = {
                        let mut state = self.state.lock().unwrap();
                        let Some(heartbeat) = state.pis.get_mut(id) else {
                            return;
                        };
                        heartbeat.not_found = 0;
                        heartbeat.transient = 0;
                        heartbeat.interval
                    };
                    self.schedule_pi(id, interval);
                }
                Err(error) => self.retry_pi(id, error).await,
            }
        })
    }
    async fn retry_pi(self: &Arc<Self>, id: &str, error: RadiusError) {
        let (recover, delay, count) = {
            let mut state = self.state.lock().unwrap();
            let Some(heartbeat) = state.pis.get_mut(id) else {
                return;
            };
            retry_decision(heartbeat, error.not_found(), (self.options.random)())
        };
        if !recover {
            if !error.not_found() {
                log_retry(&format!("Radius Pi {id}"), "heartbeat", delay, count, &error);
            }
            self.schedule_pi(id, delay);
            return;
        }
        match self.re_register_pi(id).await {
            Ok(true) => {}
            Ok(false) => {
                let delay = compute_backoff_delay_ms(1, (self.options.random)());
                eprintln!("Radius Pi {id} re-registration skipped; retrying in {delay}ms");
                self.schedule_pi(id, delay);
            }
            Err(error) => {
                let count = {
                    let mut state = self.state.lock().unwrap();
                    let Some(heartbeat) = state.pis.get_mut(id) else {
                        return;
                    };
                    heartbeat.transient += 1;
                    heartbeat.transient
                };
                let delay = compute_backoff_delay_ms(count, (self.options.random)());
                log_retry(&format!("Radius Pi {id}"), "re-registration", delay, count, &error);
                self.schedule_pi(id, delay);
            }
        }
    }
    fn re_register_machine_and_pis(self: &Arc<Self>) -> BoxFuture<'_, Result<(), RadiusError>> {
        Box::pin(async move {
            let label = self
                .state
                .lock()
                .unwrap()
                .machine
                .as_ref()
                .and_then(|v| v["label"].as_str())
                .map(str::to_owned);
            let registered = self.register_machine(label).await?;
            let interval = registered["heartbeatIntervalMs"].as_u64().unwrap_or_default();
            self.state.lock().unwrap().machine_heartbeat.interval = interval;
            self.schedule_machine(interval);
            let instances = self.coordinator().map(|c| c.list_live_instances()).unwrap_or_default();
            for instance in instances {
                let id = instance["id"].as_str().unwrap_or_default();
                if let Err(error) = self.re_register_pi(id).await {
                    eprintln!("Radius Pi {id} re-registration failed: {error}");
                }
            }
            Ok(())
        })
    }
    fn re_register_pi<'a>(self: &'a Arc<Self>, id: &'a str) -> BoxFuture<'a, Result<bool, RadiusError>> {
        Box::pin(async move {
            let coordinator = self.coordinator();
            let instance = coordinator.as_ref().and_then(|c| c.get_live_instance(id));
            let Some(instance) = instance else {
                if let Some(mut heartbeat) = self.state.lock().unwrap().pis.shift_remove(id) {
                    abort_timer(&mut heartbeat);
                }
                return Ok(false);
            };
            if self.state.lock().unwrap().machine.is_none() {
                self.re_register_machine_and_pis().await?;
                return Ok(true);
            }
            let registered = self.register_pi(instance).await?;
            if let Some(coordinator) = coordinator {
                coordinator.update_instance(registered)?;
            }
            Ok(true)
        })
    }
}
fn abort_timer(heartbeat: &mut Heartbeat) {
    if let Some(timer) = heartbeat.timer.take() {
        timer.abort();
    }
}
fn retry_decision(heartbeat: &mut Heartbeat, not_found: bool, random: f64) -> (bool, u64, u32) {
    if not_found {
        heartbeat.transient = 0;
        heartbeat.not_found += 1;
        return (heartbeat.not_found >= 3, heartbeat.interval, heartbeat.not_found);
    }
    heartbeat.transient += 1;
    (
        false,
        compute_backoff_delay_ms(heartbeat.transient, random),
        heartbeat.transient,
    )
}
fn log_retry(scope: &str, action: &str, delay: u64, count: u32, error: &RadiusError) {
    let error = if let Some(status) = error.status {
        format!("HTTP {status}: {error}")
    } else {
        error.to_string()
    };
    eprintln!("{scope} {action} failed (attempt {count}); retrying in {delay}ms: {error}");
}
pub(crate) fn copy_optional(from: &Value, to: &mut Value, keys: &[&str]) {
    for key in keys {
        if let Some(value) = from.get(*key) {
            to[*key] = value.clone();
        }
    }
}
