//! Radius presence, with injected Fetch/clock/randomness for isolated conformance.
use crate::{
    config::{Config, VERSION},
    storage::Storage,
};
use indexmap::IndexMap;
use pi_ai::request::{BoxFuture, Fetch, HttpRequest, read_body_text};
use pi_js_compat::{
    js_format, js_json as json,
    js_json::{JsString, JsValue as Value, encode_node_utf8},
};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex, Weak},
    time::Duration,
};
use tokio::task::AbortHandle;

fn nonempty<'a>(env: &'a BTreeMap<String, JsString>, key: &str) -> Option<&'a JsString> {
    env.get(key).filter(|s| !s.is_empty())
}
pub fn radius_url(env: &BTreeMap<String, JsString>) -> JsString {
    nonempty(env, "PI_RADIUS_URL")
        .cloned()
        .unwrap_or_else(|| JsString::from("https://radius.pi.dev/"))
}
pub fn radius_orchestrator_base_url(env: &BTreeMap<String, JsString>) -> Result<JsString, JsString> {
    if let Some(url) = nonempty(env, "PI_RADIUS_ORCHESTRATOR_URL") {
        return Ok(url.into());
    }
    url::Url::parse(&encode_node_utf8(&radius_url(env)))
        .and_then(|url| url.join("/v1/"))
        .map(|v| JsString::from(v.to_string()))
        .map_err(|_| JsString::from("Invalid URL"))
}
fn oauth(credential: Option<&Value>) -> Option<&Value> {
    credential.filter(|v| v["type"] == json!("oauth"))
}
pub fn radius_access_token(credential: Option<&Value>, env: &BTreeMap<String, JsString>) -> Result<JsString, JsString> {
    if let Some(access) = oauth(credential)
        .and_then(|c| c["access"].as_string())
        .filter(|s| !s.is_empty())
    {
        return Ok(access.into());
    }
    nonempty(env, "PI_RADIUS_API_KEY")
        .map(JsString::from)
        .ok_or_else(|| "Radius credentials are required in ~/.pi/agent/auth.json or PI_RADIUS_API_KEY".into())
}
pub fn radius_enabled(credential: Option<&Value>, env: &BTreeMap<String, JsString>) -> bool {
    let access = oauth(credential).map(|v| pi_js_compat::js_json::JsValue::from(&v["access"]));
    access.as_ref().is_some_and(pi_js_compat::js_json::is_truthy) || nonempty(env, "PI_RADIUS_API_KEY").is_some()
}
pub fn compute_backoff_delay_ms(failure: u32, random: f64) -> u64 {
    let exponent = failure.saturating_sub(1).min(30);
    let exponential = (1000u64.saturating_mul(1u64 << exponent)).min(30000);
    let jitter = (random * (exponential as f64 / 4.0).max(250.0)).floor() as u64;
    (exponential + jitter).min(30000)
}
pub type Environment = Arc<dyn Fn() -> BTreeMap<String, JsString> + Send + Sync>;
pub struct RadiusOptions {
    pub env: Environment,
    pub credential: Arc<dyn Fn() -> Option<Value> + Send + Sync>,
    pub now: Arc<dyn Fn() -> JsString + Send + Sync>,
    pub random: Arc<dyn Fn() -> f64 + Send + Sync>,
    pub hostname: JsString,
    pub platform: JsString,
    pub arch: JsString,
    pub pid: u32,
}
impl RadiusOptions {
    pub fn from_environment() -> Result<Self, pi_coding_agent::config::ConfigError> {
        let auth = Arc::new(pi_coding_agent::core::auth_storage::AuthStorage::create(None)?);
        Ok(Self {
            env: Arc::new(|| {
                std::env::vars()
                    .map(|(key, value)| (key, JsString::from(value)))
                    .collect()
            }),
            credential: Arc::new(move || {
                auth.reload();
                auth.get("radius")
            }),
            now: Arc::new(|| {
                JsString::from(pi_js_compat::date::to_iso_string(
                    pi_js_compat::date::js_now_millis() as i64
                ))
            }),
            random: Arc::new(|| {
                let mut bytes = [0; 8];
                getrandom::fill(&mut bytes).expect("OS random source unavailable");
                (u64::from_le_bytes(bytes) >> 11) as f64 / (1u64 << 53) as f64
            }),
            hostname: JsString::from(gethostname::gethostname().to_str().expect("OS hostname must be UTF-8")),
            platform: std::env::consts::OS.into(),
            arch: match std::env::consts::ARCH {
                "x86_64" => "x64",
                "aarch64" => "arm64",
                v => v,
            }
            .into(),
            pid: std::process::id(),
        })
    }
}
pub trait RadiusPresenceCoordinator: Send + Sync {
    fn get_live_instance(&self, id: &JsString) -> Option<Value>;
    fn list_live_instances(&self) -> Vec<Value>;
    fn update_instance(&self, instance: Value) -> Result<(), JsString>;
}
#[derive(Default)]
struct Heartbeat {
    timer: Option<AbortHandle>,
    interval: u64,
    radius_id: JsString,
    not_found: u32,
    transient: u32,
}
#[derive(Default)]
struct State {
    machine: Option<Value>,
    machine_heartbeat: Heartbeat,
    pis: IndexMap<JsString, Heartbeat>,
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
    message: JsString,
}
impl RadiusError {
    fn not_found(&self) -> bool {
        self.status == Some(404)
    }
}
impl From<JsString> for RadiusError {
    fn from(message: JsString) -> Self {
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
    async fn post(&self, path: &JsString, body: Value, read_json: bool) -> Result<Value, RadiusError> {
        let env = (self.options.env)();
        let base = radius_orchestrator_base_url(&env)?;
        let url = url::Url::parse(&encode_node_utf8(&base))
            .and_then(|url| url.join(&encode_node_utf8(path)))
            .map_err(|_| JsString::from("Invalid URL"))?;
        let token = radius_access_token((self.options.credential)().as_ref(), &env)?;
        let request = HttpRequest {
            method: "POST".into(),
            url: url.to_string(),
            headers: vec![
                (
                    "Authorization".into(),
                    pi_ai::request::header_value(&js_format!("Bearer {}", token)).map_err(|error| error.message)?,
                ),
                ("Content-Type".into(), "application/json".into()),
            ],
            body: Some(pi_js_compat::js_json::stringify_lossless(&body).into_bytes()),
        };
        let response = self.fetch.fetch(request, None).await.map_err(|e| e.message)?;
        let status = response.status;
        if (200..300).contains(&status) && !read_json {
            return Ok(Value::Null);
        }
        let text = read_body_text(response.body).await.map_err(|e| e.message)?;
        if !(200..300).contains(&status) {
            return Err(RadiusError {
                status: Some(status),
                message: js_format!("Radius request failed: {} {}", status, text),
            });
        }
        if read_json {
            pi_js_compat::js_json::parse_lossless(text).map_err(|error| error.into_message().into())
        } else {
            Ok(Value::Null)
        }
    }
    pub async fn start(self: &Arc<Self>, label: Option<JsString>) -> Result<Option<Value>, JsString> {
        if !self.enabled() {
            return Ok(None);
        }
        let registered = self.register_machine(label).await.map_err(|e| e.message)?;
        self.state.lock().unwrap().machine_heartbeat.interval = registered["heartbeatIntervalMs"]
            .as_u64()
            .ok_or_else(|| JsString::from("Radius heartbeat interval missing"))?;
        self.schedule_machine(
            registered["heartbeatIntervalMs"]
                .as_u64()
                .ok_or_else(|| JsString::from("Radius heartbeat interval missing"))?,
        );
        Ok(self.state.lock().unwrap().machine.clone())
    }
    pub async fn stop(&self) -> Result<(), JsString> {
        let machine = {
            let mut state = self.state.lock().unwrap();
            abort_timer(&mut state.machine_heartbeat);
            for (_, mut heartbeat) in state.pis.drain(..) {
                abort_timer(&mut heartbeat);
            }
            state.machine.clone()
        };
        if let Some(machine) = machine.filter(|_| self.enabled()) {
            self.disconnect(&js_format!(
                "machines/{}/disconnect",
                machine["id"].as_string().expect("registered Radius record id")
            ))
            .await?;
        }
        Ok(())
    }
    async fn disconnect(&self, path: &JsString) -> Result<(), JsString> {
        match self.post(path, json!({}), false).await {
            Ok(_) => Ok(()),
            Err(e) if e.not_found() => Ok(()),
            Err(e) => Err(e.message),
        }
    }
    pub async fn register_pi(self: &Arc<Self>, instance: Value) -> Result<Value, JsString> {
        self.register_pi_with_status(instance)
            .await
            .map_err(|error| error.message)
    }

    async fn register_pi_with_status(self: &Arc<Self>, mut instance: Value) -> Result<Value, RadiusError> {
        if !self.enabled() {
            return Ok(instance);
        }
        let machine = self
            .load_machine_identity()?
            .ok_or_else(|| JsString::from("No registered machine available for Pi registration"))?;
        let body = build_pi_registration_body(&machine, &instance, &self.options);
        let registered = self.post(&JsString::from("pis/register"), body, true).await?;
        instance["radiusPiId"] = registered["id"].clone();
        let id = instance["id"]
            .as_string()
            .ok_or_else(|| JsString::from("instance ID missing"))?
            .to_owned();
        let heartbeat = Heartbeat {
            interval: registered["heartbeatIntervalMs"]
                .as_u64()
                .ok_or_else(|| JsString::from("Radius heartbeat interval missing"))?,
            radius_id: registered["id"]
                .as_string()
                .expect("registered Radius record id")
                .into(),
            ..Default::default()
        };
        let interval = heartbeat.interval;
        if let Some(mut old) = self.state.lock().unwrap().pis.insert(id.clone(), heartbeat) {
            abort_timer(&mut old);
        }
        self.schedule_pi(&id, interval);
        Ok(instance)
    }
    pub async fn disconnect_pi(&self, instance: &Value) -> Result<(), JsString> {
        if let Some(mut heartbeat) = self
            .state
            .lock()
            .unwrap()
            .pis
            .shift_remove(instance["id"].as_string().expect("registered Radius record id"))
        {
            abort_timer(&mut heartbeat);
        }
        let Some(id) = instance["radiusPiId"].as_string().filter(|v| !v.is_empty()) else {
            return Ok(());
        };
        if !self.enabled() {
            return Ok(());
        }
        self.disconnect(&js_format!("pis/{}/disconnect", id)).await
    }
    fn load_machine_identity(&self) -> Result<Option<Value>, JsString> {
        let cached = self.state.lock().unwrap().machine.clone();
        match cached {
            Some(machine) => Ok(Some(machine)),
            None => self.storage.load_machine(),
        }
    }
    async fn register_machine(&self, label: Option<JsString>) -> Result<Value, RadiusError> {
        let existing = self.load_machine_identity()?;
        let body = build_machine_registration_body(existing.as_ref(), label.as_ref(), &self.options);
        let registered = self.post(&JsString::from("machines/register"), body, true).await?;
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
    fn schedule_pi(self: &Arc<Self>, id: &JsString, delay: u64) {
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
                    &js_format!(
                        "machines/{}/heartbeat",
                        machine["id"].as_string().expect("registered Radius record id")
                    ),
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
    fn heartbeat_pi<'a>(self: &'a Arc<Self>, id: &'a JsString) -> BoxFuture<'a, ()> {
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
            let result = self
                .post(&js_format!("pis/{}/heartbeat", radius_id), json!({}), false)
                .await;
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
    async fn retry_pi(self: &Arc<Self>, id: &JsString, error: RadiusError) {
        let (recover, delay, count) = {
            let mut state = self.state.lock().unwrap();
            let Some(heartbeat) = state.pis.get_mut(id) else {
                return;
            };
            retry_decision(heartbeat, error.not_found(), (self.options.random)())
        };
        if !recover {
            if !error.not_found() {
                log_retry(&js_format!("Radius Pi {}", id), "heartbeat", delay, count, &error);
            }
            self.schedule_pi(id, delay);
            return;
        }
        match self.re_register_pi(id).await {
            Ok(true) => {}
            Ok(false) => {
                let delay = compute_backoff_delay_ms(1, (self.options.random)());
                eprintln!(
                    "{}",
                    encode_node_utf8(&js_format!(
                        "Radius Pi {} re-registration skipped; retrying in {}ms",
                        id,
                        delay
                    ))
                );
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
                log_retry(&js_format!("Radius Pi {}", id), "re-registration", delay, count, &error);
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
                .and_then(|v| v["label"].as_string())
                .map(JsString::from);
            let registered = self.register_machine(label).await?;
            let interval = registered["heartbeatIntervalMs"]
                .as_u64()
                .ok_or_else(|| JsString::from("Radius heartbeat interval missing"))?;
            self.state.lock().unwrap().machine_heartbeat.interval = interval;
            self.schedule_machine(interval);
            let instances = match self.coordinator() {
                Some(coordinator) => coordinator.list_live_instances(),
                None => Vec::new(),
            };
            for instance in instances {
                let id = instance["id"].as_string().expect("registered Radius record id");
                if let Err(error) = self.re_register_pi(id).await {
                    eprintln!(
                        "{}",
                        encode_node_utf8(&js_format!(
                            "Radius Pi {} re-registration failed: {}",
                            id,
                            format_radius_error(&error)
                        ))
                    );
                }
            }
            Ok(())
        })
    }
    fn re_register_pi<'a>(self: &'a Arc<Self>, id: &'a JsString) -> BoxFuture<'a, Result<bool, RadiusError>> {
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
            let registered = self.register_pi_with_status(instance).await?;
            if let Some(coordinator) = coordinator {
                coordinator.update_instance(registered)?;
            }
            Ok(true)
        })
    }
}
fn build_machine_registration_body(
    existing: Option<&Value>,
    label: Option<&JsString>,
    options: &RadiusOptions,
) -> Value {
    let mut body = json!({});
    if let Some(machine) = existing {
        body["machineId"] = machine["id"].clone();
    }
    if let Some(label) = label {
        body["label"] = json!(label);
    }
    body["hostname"] = json!(options.hostname);
    body["platform"] = json!(options.platform);
    body["arch"] = json!(options.arch);
    body["version"] = json!(VERSION);
    body["capabilities"] = json!({"spawn":true,"relay":false,"iroh":false});
    body
}

fn build_pi_registration_body(machine: &Value, instance: &Value, options: &RadiusOptions) -> Value {
    let mut body = json!({"machineId":machine["id"]});
    copy_optional(instance, &mut body, &["label"]);
    body["cwd"] = instance["cwd"].clone();
    body["hostname"] = json!(options.hostname);
    body["pid"] = json!(options.pid);
    body["transport"] = json!("local-rpc");
    body["capabilities"] = json!({"rpc":true,"relay":false,"iroh":false});
    copy_optional(instance, &mut body, &["sessionId"]);
    body
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
fn format_radius_error(error: &RadiusError) -> JsString {
    match error.status {
        Some(status) => js_format!("HTTP {}: {}", status, error.message),
        None => error.message.clone(),
    }
}

fn log_retry(scope: impl Into<JsString>, action: &str, delay: u64, count: u32, error: &RadiusError) {
    let error = format_radius_error(error);
    eprintln!(
        "{}",
        encode_node_utf8(&js_format!(
            "{} {} failed (attempt {}); retrying in {}ms: {}",
            scope.into(),
            action,
            count,
            delay,
            error
        ))
    );
}
pub(crate) fn copy_optional(from: &Value, to: &mut Value, keys: &[&str]) {
    for key in keys {
        if let Some(value) = from.get(key) {
            to[*key] = value.clone();
        }
    }
}
