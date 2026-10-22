//! Instance lifecycle, persistent metadata and RPC stream subscriptions.
use indexmap::IndexMap;
use pi_js_compat::{
    js_format, js_json as json,
    js_json::{JsString, JsValue as Value, encode_node_utf8},
};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};

use crate::{
    radius::{RadiusPresence, RadiusPresenceCoordinator},
    rpc_process::{Listener, RpcProcessInstance, random_uuid},
    storage::Storage,
};

pub type SpawnProcess = Arc<dyn Fn(&JsString) -> Result<RpcProcessInstance, JsString> + Send + Sync>;
pub struct SupervisorOptions {
    pub now: Arc<dyn Fn() -> JsString + Send + Sync>,
    pub id: Arc<dyn Fn() -> Result<JsString, JsString> + Send + Sync>,
    pub spawn: SpawnProcess,
}
impl Default for SupervisorOptions {
    fn default() -> Self {
        Self {
            now: Arc::new(|| {
                JsString::from(pi_js_compat::date::to_iso_string(
                    pi_js_compat::date::js_now_millis() as i64
                ))
            }),
            id: Arc::new(random_uuid),
            spawn: Arc::new(RpcProcessInstance::spawn),
        }
    }
}
struct LiveInstance {
    record: Value,
    process: Option<RpcProcessInstance>,
    radius_id: Option<JsString>,
    session_id: Option<JsString>,
    subscribers: IndexMap<usize, Listener>,
    ui: Option<Listener>,
    event_subscription: Option<usize>,
    exit_subscription: Option<usize>,
}
type Live = Arc<Mutex<LiveInstance>>;
pub struct OrchestratorSupervisor {
    storage: Storage,
    radius: Arc<RadiusPresence>,
    options: SupervisorOptions,
    live: Mutex<IndexMap<JsString, Live>>,
    ids: AtomicUsize,
    fatal: tokio::sync::watch::Sender<Option<JsString>>,
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
    pub fn subscribe_fatal_errors(&self) -> tokio::sync::watch::Receiver<Option<JsString>> {
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
    fn find_live(&self, id: &JsString) -> Option<Live> {
        self.live.lock().unwrap().get(id).cloned()
    }
    fn persist_status(&self, live: &Live, status: &str) -> Result<(), JsString> {
        let record = {
            let mut live = live.lock().unwrap();
            live.record["status"] = json!(status);
            live.record["lastSeenAt"] = json!((self.options.now)());
            live.record.clone()
        };
        self.storage.upsert_instance(record)
    }
    fn persist_updates(&self, live: &Live, updates: Value) -> Result<(), JsString> {
        let record = {
            let mut live = live.lock().unwrap();
            for (key, value) in updates.as_object().ok_or("updates must be an object")? {
                if value.is_null() {
                    live.record.as_object_mut().unwrap().shift_remove(key);
                } else {
                    live.record
                        .as_object_mut()
                        .expect("instance record object")
                        .insert(key.clone(), value.clone());
                }
            }
            if let Some(id) = updates["radiusPiId"].as_string() {
                live.radius_id = Some(id.into());
            }
            if let Some(id) = updates["sessionId"].as_string() {
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
        let event_id = process.on_event(event_listener(live));
        let this = Arc::downgrade(self);
        let exited = Arc::downgrade(live);
        let exit_id = process.on_exit(Arc::new(move |_error| {
            if let (Some(this), Some(live)) = (this.upgrade(), exited.upgrade()) {
                tokio::spawn(async move {
                    if let Err(error) = this.unexpected_exit(live).await {
                        eprintln!("RPC exit cleanup: {}", encode_node_utf8(&error));
                    }
                });
            }
        }));
        process.set_ui_request_handler(Some(ui_listener(live)));
        let mut live = live.lock().unwrap();
        live.event_subscription = Some(event_id);
        live.exit_subscription = Some(exit_id);
    }
    async fn unexpected_exit(&self, live: Live) -> Result<(), JsString> {
        let record = live.lock().unwrap().record.clone();
        let id = record["id"].as_string().expect("instance record id");
        if !self.find_live(id).is_some_and(|l| Arc::ptr_eq(&l, &live))
            || matches!(record["status"].as_string(), Some(tag) if tag == "stopping" || tag == "stopped")
        {
            return Ok(());
        }
        self.persist_status(&live, "error")?;
        Self::clear_bindings(&live);
        live.lock().unwrap().process = None;
        if live.lock().unwrap().radius_id.is_some() {
            match self.radius.disconnect_pi(&record).await {
                Ok(()) => self.persist_updates(&live, json!({"radiusPiId":null}))?,
                Err(error) => eprintln!(
                    "{}",
                    encode_node_utf8(&js_format!("Failed to disconnect Radius Pi {}: Error: {}", id, error))
                ),
            }
        }
        self.live.lock().unwrap().shift_remove(id);
        Ok(())
    }
    async fn sync_record(&self, live: &Live) -> Result<(), JsString> {
        let process = live.lock().unwrap().process.clone();
        let Some(process) = process else {
            return self.persist_updates(live, json!({}));
        };
        let response = process.send(json!({"type":"get_state"})).await?;
        if response["success"] != json!(true)
            || response["command"] != json!("get_state")
            || response.get("data").is_none()
        {
            return self.persist_updates(live, json!({}));
        }
        self.persist_updates(
            live,
            json!({"sessionId":response["data"]["sessionId"],"sessionFile":response["data"]["sessionFile"]}),
        )
    }
    async fn cleanup_resources(&self, live: &Live) -> Result<(), JsString> {
        let process = live.lock().unwrap().process.clone();
        Self::clear_bindings(live);
        if live.lock().unwrap().radius_id.is_some() {
            let record = live.lock().unwrap().record.clone();
            self.radius.disconnect_pi(&record).await?;
            let mut live = live.lock().unwrap();
            live.radius_id = None;
            live.record
                .as_object_mut()
                .unwrap()
                .shift_remove(&JsString::from("radiusPiId"));
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
    pub fn update_instance(&self, instance: Value) -> Result<(), JsString> {
        if let Some(live) = self.find_live(instance["id"].as_string().expect("instance record id")) {
            let mut live = live.lock().unwrap();
            live.record = instance.clone();
            live.radius_id = instance["radiusPiId"].as_string().map(JsString::from);
            live.session_id = instance["sessionId"].as_string().map(JsString::from);
        }
        self.storage.upsert_instance(instance)
    }
    pub fn get_live_instance(&self, id: &JsString) -> Option<Value> {
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
    pub async fn recover_after_restart(&self) -> Result<(), JsString> {
        let timestamp = (self.options.now)();
        let mut instances = self.storage.load_instances()?;
        for instance in &mut instances {
            if matches!(instance["status"].as_string(), Some(tag) if tag == "online" || tag == "starting") {
                instance["status"] = json!("stopped");
            }
            instance["lastSeenAt"] = json!(timestamp);
        }
        for instance in &instances {
            self.radius.disconnect_pi(instance).await?;
        }
        self.storage.save_instances(&instances)
    }
    pub fn list_instances(&self) -> Result<Vec<Value>, JsString> {
        self.storage.load_instances()
    }
    pub fn get_instance(&self, id: &JsString) -> Result<Option<Value>, JsString> {
        if let Some(instance) = self.get_live_instance(id) {
            return Ok(Some(instance));
        }
        self.storage.get_instance(id)
    }
    pub async fn spawn_instance(self: &Arc<Self>, cwd: JsString, label: Option<JsString>) -> Result<Value, JsString> {
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
        let result = self.acquire_resources(&live, &cwd).await;
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
    async fn acquire_resources(self: &Arc<Self>, live: &Live, cwd: &JsString) -> Result<(), JsString> {
        let process = (self.options.spawn)(cwd)?;
        self.bind_process(live, process);
        self.sync_record(live).await?;
        let record = live.lock().unwrap().record.clone();
        let registered = self.radius.register_pi(record).await?;
        self.persist_updates(live, json!({"radiusPiId":registered["radiusPiId"]}))?;
        self.persist_status(live, "online")
    }
    pub async fn stop_instance(&self, id: &JsString) -> Result<Option<Value>, JsString> {
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
    pub async fn handle_rpc(&self, id: &JsString, command: Value) -> Result<Option<Value>, JsString> {
        let Some(live) = self.find_live(id) else {
            return Ok(None);
        };
        self.send_rpc(&live, command).await
    }
    async fn send_rpc(&self, live: &Live, command: Value) -> Result<Option<Value>, JsString> {
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
        id: &JsString,
        on_event: Listener,
        on_ui: Listener,
    ) -> Option<Arc<SupervisorStream>> {
        let live = self.find_live(id)?;
        let process = live.lock().unwrap().process.clone()?;
        let token = {
            let mut live = live.lock().unwrap();
            let token = live
                .subscribers
                .iter()
                .find(|(_, existing)| Arc::ptr_eq(existing, &on_event))
                .map(|(id, _)| *id)
                .unwrap_or_else(|| self.ids.fetch_add(1, Ordering::Relaxed));
            live.subscribers.entry(token).or_insert(on_event);
            live.ui = Some(on_ui.clone());
            token
        };
        Some(Arc::new(SupervisorStream {
            supervisor: self.clone(),
            live,
            process,
            token,
            on_ui,
        }))
    }
    pub async fn shutdown(&self) -> Result<(), JsString> {
        let ids = self.live.lock().unwrap().keys().cloned().collect::<Vec<_>>();
        for id in ids {
            self.stop_instance(&id).await?;
        }
        Ok(())
    }
}
impl RadiusPresenceCoordinator for OrchestratorSupervisor {
    fn get_live_instance(&self, id: &JsString) -> Option<Value> {
        self.get_live_instance(id)
    }
    fn list_live_instances(&self) -> Vec<Value> {
        self.list_live_instances()
    }
    fn update_instance(&self, instance: Value) -> Result<(), JsString> {
        self.update_instance(instance)
    }
}
fn event_listener(live: &Live) -> Listener {
    let live = Arc::downgrade(live);
    Arc::new(move |event| {
        let Some(live) = live.upgrade() else {
            return;
        };
        let subscribers = live.lock().unwrap().subscribers.values().cloned().collect::<Vec<_>>();
        for subscriber in subscribers {
            subscriber(event.clone());
        }
    })
}
fn ui_listener(live: &Live) -> Listener {
    let live = Arc::downgrade(live);
    Arc::new(move |request| {
        let handler = live.upgrade().and_then(|live| live.lock().unwrap().ui.clone());
        if let Some(handler) = handler {
            handler(request);
        }
    })
}

fn refresh_metadata(command: &Value) -> bool {
    matches!(
        command["type"].as_string(),
        Some(tag) if tag == "new_session" || tag == "switch_session" || tag == "fork" || tag == "clone" || tag == "set_session_name" || tag == "prompt"
    )
}
pub struct SupervisorStream {
    supervisor: Arc<OrchestratorSupervisor>,
    live: Live,
    process: RpcProcessInstance,
    token: usize,
    on_ui: Listener,
}
impl SupervisorStream {
    pub async fn handle_rpc(&self, command: Value) -> Result<Value, JsString> {
        let response = self.process.send(command.clone()).await?;
        if refresh_metadata(&command) {
            self.supervisor.sync_record(&self.live).await?;
        }
        Ok(response)
    }
    pub async fn handle_ui_response(&self, response: Value) -> Result<(), JsString> {
        self.process.handle_ui_response(response).await
    }
    pub fn close(&self) {
        let mut live = self.live.lock().unwrap();
        if live
            .ui
            .as_ref()
            .is_some_and(|callback| Arc::ptr_eq(callback, &self.on_ui))
        {
            live.ui = None;
        }
        live.subscribers.shift_remove(&self.token);
    }
}
