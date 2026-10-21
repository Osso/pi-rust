//! Instance lifecycle, persistent metadata and RPC stream subscriptions.
use indexmap::IndexMap;
use serde_json::{Value, json};
use std::{
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};

use crate::{
    radius::{RadiusPresence, RadiusPresenceCoordinator},
    rpc_process::{Listener, RpcProcessInstance, random_uuid},
    storage::Storage,
};

pub type SpawnProcess = Arc<dyn Fn(&Path) -> Result<RpcProcessInstance, String> + Send + Sync>;
pub struct SupervisorOptions {
    pub now: Arc<dyn Fn() -> String + Send + Sync>,
    pub id: Arc<dyn Fn() -> Result<String, String> + Send + Sync>,
    pub spawn: SpawnProcess,
}
impl Default for SupervisorOptions {
    fn default() -> Self {
        Self {
            now: Arc::new(|| pi_js_compat::date::to_iso_string(pi_js_compat::date::js_now_millis() as i64)),
            id: Arc::new(random_uuid),
            spawn: Arc::new(RpcProcessInstance::spawn),
        }
    }
}
struct LiveInstance {
    record: Value,
    process: Option<RpcProcessInstance>,
    radius_id: Option<String>,
    session_id: Option<String>,
    subscribers: IndexMap<usize, Listener>,
    ui: Option<(usize, Listener)>,
    event_subscription: Option<usize>,
    exit_subscription: Option<usize>,
}
type Live = Arc<Mutex<LiveInstance>>;
pub struct OrchestratorSupervisor {
    storage: Storage,
    radius: Arc<RadiusPresence>,
    options: SupervisorOptions,
    live: Mutex<IndexMap<String, Live>>,
    ids: AtomicUsize,
    fatal: tokio::sync::watch::Sender<Option<String>>,
}

impl OrchestratorSupervisor {
    pub fn new(storage: Storage, radius: Arc<RadiusPresence>, options: SupervisorOptions) -> Arc<Self> {
        let supervisor = Arc::new(Self {
            storage,
            radius: radius.clone(),
            options,
            live: Mutex::new(IndexMap::new()),
            ids: AtomicUsize::new(0),
            fatal: tokio::sync::watch::channel(None).0,
        });
        let coordinator: Arc<dyn RadiusPresenceCoordinator> = supervisor.clone();
        radius.set_coordinator(Arc::downgrade(&coordinator));
        supervisor
    }
    pub fn subscribe_fatal_errors(&self) -> tokio::sync::watch::Receiver<Option<String>> {
        self.fatal.subscribe()
    }
    fn observe_fatal_errors(self: &Arc<Self>, process: &RpcProcessInstance) {
        let mut errors = process.subscribe_fatal_errors();
        let supervisor = Arc::downgrade(self);
        tokio::spawn(async move {
            loop {
                let error = errors.borrow_and_update().clone();
                if let Some(error) = error {
                    if let Some(supervisor) = supervisor.upgrade() {
                        supervisor.fatal.send_replace(Some(error));
                    }
                    return;
                }
                if errors.changed().await.is_err() {
                    return;
                }
            }
        });
    }
    fn find_live(&self, id: &str) -> Option<Live> {
        self.live.lock().unwrap().get(id).cloned()
    }
    fn persist_status(&self, live: &Live, status: &str) -> Result<(), String> {
        let record = {
            let mut live = live.lock().unwrap();
            live.record["status"] = json!(status);
            live.record["lastSeenAt"] = json!((self.options.now)());
            live.record.clone()
        };
        self.storage.upsert_instance(record)
    }
    fn persist_updates(&self, live: &Live, updates: Value) -> Result<(), String> {
        let record = {
            let mut live = live.lock().unwrap();
            for (key, value) in updates.as_object().ok_or("updates must be an object")? {
                if value.is_null() {
                    live.record.as_object_mut().unwrap().remove(key);
                } else {
                    live.record[key] = value.clone();
                }
            }
            if let Some(id) = updates["radiusPiId"].as_str() {
                live.radius_id = Some(id.into());
            }
            if let Some(id) = updates["sessionId"].as_str() {
                live.session_id = Some(id.into());
            }
            live.record["lastSeenAt"] = json!((self.options.now)());
            live.record.clone()
        };
        self.storage.upsert_instance(record)
    }
    fn clear_bindings(live: &Live) {
        let mut live = live.lock().unwrap();
        if let Some(process) = &live.process {
            if let Some(id) = live.event_subscription {
                process.remove_event_listener(id);
            }
            if let Some(id) = live.exit_subscription {
                process.remove_exit_listener(id);
            }
            process.set_ui_request_handler(None);
        }
        live.event_subscription = None;
        live.exit_subscription = None;
        live.ui = None;
    }
    fn bind_process(self: &Arc<Self>, live: &Live, process: RpcProcessInstance) {
        Self::clear_bindings(live);
        live.lock().unwrap().process = Some(process.clone());
        self.observe_fatal_errors(&process);
        let events = Arc::downgrade(live);
        let event_id = process.on_event(Arc::new(move |event| {
            let subscribers = events
                .upgrade()
                .map(|l| l.lock().unwrap().subscribers.values().cloned().collect::<Vec<_>>())
                .unwrap_or_default();
            for subscriber in subscribers {
                subscriber(event.clone());
            }
        }));
        let this = Arc::downgrade(self);
        let exited = Arc::downgrade(live);
        let exit_id = process.on_exit(Arc::new(move |_error| {
            if let (Some(this), Some(live)) = (this.upgrade(), exited.upgrade()) {
                tokio::spawn(async move {
                    if let Err(error) = this.unexpected_exit(live).await {
                        eprintln!("RPC exit cleanup: {error}");
                    }
                });
            }
        }));
        let ui = Arc::downgrade(live);
        process.set_ui_request_handler(Some(Arc::new(move |request| {
            let handler = ui
                .upgrade()
                .and_then(|l| l.lock().unwrap().ui.as_ref().map(|(_, cb)| cb.clone()));
            if let Some(handler) = handler {
                handler(request);
            }
        })));
        let mut live = live.lock().unwrap();
        live.event_subscription = Some(event_id);
        live.exit_subscription = Some(exit_id);
    }
    async fn unexpected_exit(&self, live: Live) -> Result<(), String> {
        let record = live.lock().unwrap().record.clone();
        let id = record["id"].as_str().unwrap_or_default();
        if !self.find_live(id).is_some_and(|l| Arc::ptr_eq(&l, &live))
            || matches!(record["status"].as_str(), Some("stopping" | "stopped"))
        {
            return Ok(());
        }
        self.persist_status(&live, "error")?;
        Self::clear_bindings(&live);
        live.lock().unwrap().process = None;
        if live.lock().unwrap().radius_id.is_some() {
            match self.radius.disconnect_pi(&record).await {
                Ok(()) => self.persist_updates(&live, json!({"radiusPiId":null}))?,
                Err(error) => eprintln!("Failed to disconnect Radius Pi {id}: Error: {error}"),
            }
        }
        self.live.lock().unwrap().shift_remove(id);
        Ok(())
    }
    async fn sync_record(&self, live: &Live) -> Result<(), String> {
        let process = live.lock().unwrap().process.clone();
        let Some(process) = process else {
            return self.persist_updates(live, json!({}));
        };
        let response = process.send(json!({"type":"get_state"})).await?;
        if response["success"] != true || response["command"] != "get_state" || response.get("data").is_none() {
            return self.persist_updates(live, json!({}));
        }
        self.persist_updates(
            live,
            json!({"sessionId":response["data"]["sessionId"],"sessionFile":response["data"]["sessionFile"]}),
        )
    }
    async fn cleanup_resources(&self, live: &Live) -> Result<(), String> {
        let process = live.lock().unwrap().process.clone();
        Self::clear_bindings(live);
        if live.lock().unwrap().radius_id.is_some() {
            let record = live.lock().unwrap().record.clone();
            self.radius.disconnect_pi(&record).await?;
            let mut live = live.lock().unwrap();
            live.radius_id = None;
            live.record.as_object_mut().unwrap().remove("radiusPiId");
            live.record["lastSeenAt"] = json!((self.options.now)());
        }
        {
            let mut live = live.lock().unwrap();
            live.session_id = None;
            if process.is_some() {
                live.process = None;
            }
        }
        if let Some(process) = process {
            process.dispose().await?;
        }
        Ok(())
    }
    pub fn update_instance(&self, instance: Value) -> Result<(), String> {
        if let Some(live) = self.find_live(instance["id"].as_str().unwrap_or_default()) {
            let mut live = live.lock().unwrap();
            live.record = instance.clone();
            live.radius_id = instance["radiusPiId"].as_str().map(str::to_owned);
            live.session_id = instance["sessionId"].as_str().map(str::to_owned);
        }
        self.storage.upsert_instance(instance)
    }
    pub fn get_live_instance(&self, id: &str) -> Option<Value> {
        self.find_live(id).map(|l| l.lock().unwrap().record.clone())
    }
    pub fn list_live_instances(&self) -> Vec<Value> {
        self.live
            .lock()
            .unwrap()
            .values()
            .map(|l| l.lock().unwrap().record.clone())
            .collect()
    }
    pub async fn recover_after_restart(&self) -> Result<(), String> {
        let timestamp = (self.options.now)();
        let mut instances = self.storage.load_instances()?;
        for instance in &mut instances {
            if matches!(instance["status"].as_str(), Some("online" | "starting")) {
                instance["status"] = json!("stopped");
            }
            instance["lastSeenAt"] = json!(timestamp);
        }
        for instance in &instances {
            self.radius.disconnect_pi(instance).await?;
        }
        self.storage.save_instances(&instances)
    }
    pub fn list_instances(&self) -> Result<Vec<Value>, String> {
        self.storage.load_instances()
    }
    pub fn get_instance(&self, id: &str) -> Result<Option<Value>, String> {
        if let Some(instance) = self.get_live_instance(id) {
            return Ok(Some(instance));
        }
        self.storage.get_instance(id)
    }
    pub async fn spawn_instance(self: &Arc<Self>, cwd: String, label: Option<String>) -> Result<Value, String> {
        let now = (self.options.now)();
        let id = (self.options.id)()?;
        let mut record = json!({"id":id,"status":"starting","cwd":cwd,"createdAt":now,"lastSeenAt":now});
        if let Some(label) = label {
            record["label"] = json!(label);
        }
        let live = Arc::new(Mutex::new(LiveInstance {
            record: record.clone(),
            process: None,
            radius_id: None,
            session_id: None,
            subscribers: IndexMap::new(),
            ui: None,
            event_subscription: None,
            exit_subscription: None,
        }));
        self.live.lock().unwrap().insert(id.clone(), live.clone());
        self.storage.upsert_instance(record)?;
        let result = self.acquire_resources(&live, Path::new(&cwd)).await;
        if let Err(error) = result {
            self.persist_status(&live, "error")?;
            let cleanup = self.cleanup_resources(&live).await;
            self.persist_status(&live, "stopped")?;
            self.live.lock().unwrap().shift_remove(&id);
            cleanup?;
            return Err(error);
        }
        let record = live.lock().unwrap().record.clone();
        Ok(record)
    }
    async fn acquire_resources(self: &Arc<Self>, live: &Live, cwd: &Path) -> Result<(), String> {
        let process = (self.options.spawn)(cwd)?;
        self.bind_process(live, process);
        self.sync_record(live).await?;
        let record = live.lock().unwrap().record.clone();
        let registered = self.radius.register_pi(record).await?;
        self.persist_updates(live, json!({"radiusPiId":registered["radiusPiId"]}))?;
        self.persist_status(live, "online")
    }
    pub async fn stop_instance(&self, id: &str) -> Result<Option<Value>, String> {
        let Some(live) = self.find_live(id) else {
            return Ok(None);
        };
        self.persist_status(&live, "stopping")?;
        let result = self.cleanup_resources(&live).await;
        let record = {
            let mut live = live.lock().unwrap();
            live.record["status"] = json!("stopped");
            live.record["lastSeenAt"] = json!((self.options.now)());
            live.record.clone()
        };
        self.live.lock().unwrap().shift_remove(id);
        self.storage.remove_instance(id)?;
        result?;
        Ok(Some(record))
    }
    pub async fn handle_rpc(&self, id: &str, command: Value) -> Result<Option<Value>, String> {
        let Some(live) = self.find_live(id) else {
            return Ok(None);
        };
        self.send_rpc(&live, command).await
    }
    async fn send_rpc(&self, live: &Live, command: Value) -> Result<Option<Value>, String> {
        let process = live.lock().unwrap().process.clone();
        let Some(process) = process else {
            return Ok(None);
        };
        let response = process.send(command.clone()).await?;
        if refresh_metadata(&command) {
            self.sync_record(live).await?;
        }
        Ok(Some(response))
    }
    pub fn open_rpc_stream(
        self: &Arc<Self>,
        id: &str,
        on_event: Listener,
        on_ui: Listener,
    ) -> Option<Arc<SupervisorStream>> {
        let live = self.find_live(id)?;
        let process = live.lock().unwrap().process.clone()?;
        let token = self.ids.fetch_add(1, Ordering::Relaxed);
        {
            let mut live = live.lock().unwrap();
            live.subscribers.insert(token, on_event);
            live.ui = Some((token, on_ui));
        }
        Some(Arc::new(SupervisorStream {
            supervisor: self.clone(),
            live,
            process,
            token,
        }))
    }
    pub async fn shutdown(&self) -> Result<(), String> {
        let ids = self.live.lock().unwrap().keys().cloned().collect::<Vec<_>>();
        for id in ids {
            self.stop_instance(&id).await?;
        }
        Ok(())
    }
}
impl RadiusPresenceCoordinator for OrchestratorSupervisor {
    fn get_live_instance(&self, id: &str) -> Option<Value> {
        self.get_live_instance(id)
    }
    fn list_live_instances(&self) -> Vec<Value> {
        self.list_live_instances()
    }
    fn update_instance(&self, instance: Value) -> Result<(), String> {
        self.update_instance(instance)
    }
}
fn refresh_metadata(command: &Value) -> bool {
    matches!(
        command["type"].as_str(),
        Some("new_session" | "switch_session" | "fork" | "clone" | "set_session_name" | "prompt")
    )
}
pub struct SupervisorStream {
    supervisor: Arc<OrchestratorSupervisor>,
    live: Live,
    process: RpcProcessInstance,
    token: usize,
}
impl SupervisorStream {
    pub async fn handle_rpc(&self, command: Value) -> Result<Value, String> {
        let response = self.process.send(command.clone()).await?;
        if refresh_metadata(&command) {
            self.supervisor.sync_record(&self.live).await?;
        }
        Ok(response)
    }
    pub async fn handle_ui_response(&self, response: Value) -> Result<(), String> {
        self.process.handle_ui_response(response).await
    }
    pub fn close(&self) {
        let mut live = self.live.lock().unwrap();
        if live.ui.as_ref().is_some_and(|(id, _)| *id == self.token) {
            live.ui = None;
        }
        live.subscribers.shift_remove(&self.token);
    }
}
