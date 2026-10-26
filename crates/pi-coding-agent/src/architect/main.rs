//! Port of `packages/coding-agent/src/architect/main.ts`.
//!

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use pi_agent_core::harness::types::BeforeToolCallResult;
use pi_ai::request::BoxFuture;
use pi_js_compat::abort::AbortSignal;
use pi_js_compat::js_format;
use pi_js_compat::js_json::{JsString, JsValue};
use pi_js_compat::number::parse_number;
use tokio::sync::Notify;

use crate::architect::observer::ArchitectObserver;
use crate::architect::project_spec::register_architect_project_spec_tool;
use crate::architect::prompt::{ARCHITECT_SYSTEM_PROMPT, build_architect_prompt};
use crate::bundled_extensions::agents_core::runtime::MultiAgentExtensionOptions;
use crate::bundled_extensions::agents_mailbox::agents_mailbox_extension;
use crate::bundled_extensions::bwrap::{BwrapExtensionOptions, bwrap_extension_factory};
use crate::core::agent_session::{AgentSession, AgentSessionEvent, MultiAgentRuntimeRole, PromptOptions};
use crate::core::extensions::loader::ExtensionFactory;
use crate::core::extensions::types::{
    BeforeAgentStartEvent, BeforeAgentStartEventResult, ExtensionContext, HandlerError, ToolCallEvent,
    ToolCallEventResult, ToolGateHandle,
};
use crate::core::model_registry::ModelRegistry;
use crate::core::multi_agent_store::{MultiAgentStore, MultiAgentStoreOptions};
use crate::core::resident_console_transport::{
    ResidentConsoleHost, ResidentConsoleServer, ResidentConsoleServerOptions, ResidentConsoleService,
    ResidentConsoleSnapshot, Unsubscribe,
};
use crate::core::resident_session::{ResidentRole, prepare_resident_session_file};
use crate::core::resource_loader::{InlineExtension, RulesScope};
use crate::core::sdk::{CreateAgentSessionOptions, create_agent_session};
use crate::core::session_control_db::{archive_session, get_control_db_path};
use crate::core::session_manager::{SessionEntry, SessionManager, SetMetadataControlDbPathOptions};
use crate::core::settings_manager::{Settings, SettingsManager, SettingsManagerCreateOptions};

pub const OBSERVER_INTERVAL_MS: u64 = 30_000;
pub const REQUEST_CLAIM_RENEW_INTERVAL_MS: u64 = 30_000;
pub const ARCHITECT_SESSION_ID: &str = "architect";
pub const ARCHITECT_REQUEST_THREAD_PREFIX: &str = "architect-request:";
pub const ARCHITECT_RULES_SCOPE: &str = "architect";
/// `ask_architect`, `contact_parent`, then `SUPERVISOR_ONLY_TOOL_NAMES`.
pub const ARCHITECT_EXCLUDED_TOOL_NAMES: [&str; 4] =
    ["ask_architect", "contact_parent", "ask_supervisor", "manage_goal"];

/// One queued console prompt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchitectConsolePrompt {
    pub id: JsString,
    pub text: JsString,
}

/// FIFO queue that wakes a waiting Architect loop when a prompt arrives.
#[derive(Debug, Default)]
pub struct ArchitectConsolePromptQueue {
    prompts: Mutex<VecDeque<ArchitectConsolePrompt>>,
    waiters: Notify,
}

impl ArchitectConsolePromptQueue {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn enqueue(&self, text: JsString, id: JsString) {
        self.prompts
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push_back(ArchitectConsolePrompt { id, text });
        self.waiters.notify_waiters();
    }

    pub fn take(&self) -> Option<ArchitectConsolePrompt> {
        self.prompts
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .pop_front()
    }

    pub fn is_empty(&self) -> bool {
        self.prompts
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .is_empty()
    }

    /// Wait for a prompt, the abort signal, or the observer interval.
    pub async fn wait(&self, signal: &AbortSignal, timeout_ms: u64) {
        if signal.aborted() || !self.is_empty() {
            return;
        }
        tokio::select! {
            () = self.waiters.notified() => {}
            _ = signal.wait() => {}
            () = tokio::time::sleep(Duration::from_millis(timeout_ms)) => {}
        }
    }
}

/// `createArchitectConsoleSnapshot`.
pub fn create_architect_console_snapshot(
    cwd: &JsString,
    generation: u64,
    session_id: Option<&JsString>,
    branch: Vec<SessionEntry>,
) -> ResidentConsoleSnapshot<SessionEntry> {
    ResidentConsoleSnapshot {
        service: ResidentConsoleService::Architect,
        session_id: session_id.cloned().unwrap_or_else(|| ARCHITECT_SESSION_ID.into()),
        cwd: cwd.clone(),
        generation,
        identity: None,
        branch,
    }
}

/// `createArchitectSettingsManager`.
pub fn create_architect_settings_manager() -> SettingsManager {
    let mut settings = Settings::new();
    settings.insert("sandboxProfile".into(), JsValue::String("read-only".into()));
    SettingsManager::in_memory(settings, SettingsManagerCreateOptions { project_trusted: None })
}

/// `createArchitectMultiAgentStore`.
pub fn create_architect_multi_agent_store(
    session_manager: &mut SessionManager,
    control_db_path: Option<&JsString>,
) -> Result<MultiAgentStore, pi_ai::request::CallbackError> {
    let control_db_path = control_db_path
        .cloned()
        .map(Ok)
        .unwrap_or_else(get_control_db_path_default)?;
    session_manager
        .set_metadata_control_db_path(
            Some(&control_db_path),
            SetMetadataControlDbPathOptions {
                index_message_text: Some(false),
                cwd_override: None,
            },
        )
        .map_err(pi_ai::request::CallbackError::from)?;
    if let Some(session_path) = session_manager.get_session_file() {
        archive_session(&control_db_path, session_path).map_err(pi_ai::request::CallbackError::from)?;
    }
    MultiAgentStore::from_session_manager(session_manager, MultiAgentStoreOptions::default())
        .map_err(pi_ai::request::CallbackError::from)
}

/// `getControlDbPath()`: derived from the process state root, so it is well-formed text.
fn get_control_db_path_default() -> Result<JsString, pi_ai::request::CallbackError> {
    Ok(get_control_db_path(
        &crate::config::get_user_state_root().map_err(pi_ai::request::CallbackError::from)?,
    ))
}

/// `completeSentArchitectRequest`.
pub trait ArchitectRequestCompleter {
    fn complete_request(&self, request_id: i64, sender_session_id: &JsString) -> Result<(), JsString>;
}

impl ArchitectRequestCompleter for ArchitectObserver {
    fn complete_request(&self, request_id: i64, sender_session_id: &JsString) -> Result<(), JsString> {
        ArchitectObserver::complete_request(self, request_id, sender_session_id)
            .map_err(|error| error.message().clone())
    }
}

/// `completeSentArchitectRequest`. The id suffix is parsed with `Number`; a suffix with an unpaired
/// surrogate is not numeric, so it is `NaN` there and no request here.
pub fn complete_sent_architect_request(
    observer: &dyn ArchitectRequestCompleter,
    input: &SentArchitectMessage,
) -> Result<(), JsString> {
    if input.to_agent_id != "main" {
        return Ok(());
    }
    let Some(thread_id) = input.thread_id.as_ref() else {
        return Ok(());
    };
    let Some(suffix) = thread_id.strip_prefix(ARCHITECT_REQUEST_THREAD_PREFIX) else {
        return Ok(());
    };
    let Ok(suffix) = suffix.to_utf8() else {
        return Ok(());
    };
    let request_id = parse_number(&suffix);
    if !is_safe_integer(request_id) || request_id < 1.0 {
        return Ok(());
    }
    observer.complete_request(request_id as i64, &input.to_session_id)
}

fn is_safe_integer(value: f64) -> bool {
    value.is_finite() && value.fract() == 0.0 && value.abs() <= 9_007_199_254_740_991.0
}

/// One outbound session message observed by the mailbox extension.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SentArchitectMessage {
    pub thread_id: Option<JsString>,
    pub to_agent_id: JsString,
    pub to_session_id: JsString,
}

/// `blockArchitectGlobalBroadcast`.
pub fn block_architect_global_broadcast(tool_name: &str) -> Option<BeforeToolCallResult> {
    let reason = match tool_name {
        "channel_post" => "Pi Architect must use direct agent messaging, not channel_post.",
        "list_sessions" => "Pi Architect must use its bounded structured observer snapshot, not list_sessions.",
        _ => return None,
    };
    Some(BeforeToolCallResult {
        block: Some(true),
        reason: Some(reason.to_owned().into()),
    })
}

/// Observer surface used by the cycle.
pub trait ArchitectCycleObserver {
    fn observe(&self) -> Result<Option<crate::architect::observer::ArchitectObservation>, JsString>;
    fn renew_requests(&self, _request_ids: &[i64]) -> Result<(), JsString> {
        Ok(())
    }
}

impl ArchitectCycleObserver for ArchitectObserver {
    fn observe(&self) -> Result<Option<crate::architect::observer::ArchitectObservation>, JsString> {
        ArchitectObserver::observe(self).map_err(|error| error.message().clone())
    }

    fn renew_requests(&self, request_ids: &[i64]) -> Result<(), JsString> {
        ArchitectObserver::renew_requests(self, request_ids).map_err(|error| error.message().clone())
    }
}

/// `runArchitectCycle`.
pub async fn run_architect_cycle<O, P, PFut>(observer: Arc<O>, prompt: P) -> Result<(), JsString>
where
    O: ArchitectCycleObserver + Send + Sync + 'static,
    P: Fn(JsString) -> PFut,
    PFut: std::future::Future<Output = Result<(), JsString>>,
{
    let Some(observation) = observer.observe()? else {
        return Ok(());
    };
    let request_ids: Vec<i64> = observation.requests.iter().map(|request| request.id).collect();
    let prompt = prompt(build_architect_prompt(&observation).into());
    if request_ids.is_empty() {
        return prompt.await;
    }
    tokio::pin!(prompt);
    let period = Duration::from_millis(REQUEST_CLAIM_RENEW_INTERVAL_MS);
    let mut interval = tokio::time::interval_at(tokio::time::Instant::now() + period, period);
    loop {
        tokio::select! {
            result = &mut prompt => return result,
            _ = interval.tick() => observer.renew_requests(&request_ids)?,
        }
    }
}

/// `createArchitectStopHandler`.
pub fn create_architect_stop_handler(
    abort_controller: pi_js_compat::abort::AbortController,
    abort_session: Arc<dyn Fn() -> BoxFuture<'static, ()> + Send + Sync>,
    exit: Arc<dyn Fn(i32) + Send + Sync>,
) -> impl Fn() + Send + Sync {
    let stopping = AtomicBool::new(false);
    move || {
        if stopping.swap(true, Ordering::SeqCst) {
            return;
        }
        abort_controller.abort(None);
        let abort_session = Arc::clone(&abort_session);
        tokio::spawn(async move {
            abort_session().await;
        });
        let exit = Arc::clone(&exit);
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(5)).await;
            exit(0);
        });
    }
}

/// `waitForArchitectInterval`.
pub async fn wait_for_architect_interval(signal: &AbortSignal) {
    tokio::select! {
        () = tokio::time::sleep(Duration::from_millis(OBSERVER_INTERVAL_MS)) => {}
        _ = signal.wait() => {}
    }
}

/// Console server surface used by the service loop.
pub trait ArchitectConsoleServer {
    fn start(&self) -> BoxFuture<'static, Result<(), JsString>>;
    fn close(&self) -> BoxFuture<'static, Result<(), JsString>>;
}

/// `runArchitectServiceLoop`.
pub async fn run_architect_service_loop<O, P, PFut>(
    console_server: Option<Arc<dyn ArchitectConsoleServer>>,
    observer: Arc<O>,
    prompt: P,
    prompt_queue: Arc<ArchitectConsolePromptQueue>,
    signal: AbortSignal,
) -> Result<(), JsString>
where
    O: ArchitectCycleObserver + Send + Sync + 'static,
    P: Fn(JsString) -> PFut,
    PFut: std::future::Future<Output = Result<(), JsString>>,
{
    if let Some(console_server) = &console_server {
        console_server.start().await?;
    }
    let result = async {
        while !signal.aborted() {
            run_architect_cycle(Arc::clone(&observer), &prompt).await?;
            let mut console_prompt = prompt_queue.take();
            while let Some(prompt_entry) = console_prompt {
                if signal.aborted() {
                    break;
                }
                prompt(prompt_entry.text).await?;
                console_prompt = prompt_queue.take();
            }
            prompt_queue.wait(&signal, OBSERVER_INTERVAL_MS).await;
        }
        Ok(())
    }
    .await;
    if let Some(console_server) = &console_server {
        console_server.close().await?;
    }
    result
}

/// Input for [`create_architect_console_server`].
pub struct CreateArchitectConsoleServerInput {
    pub control_db_path: JsString,
    pub cwd: JsString,
    pub generation: u64,
    pub prompt_queue: Arc<ArchitectConsolePromptQueue>,
    pub session: AgentSession,
}

struct ArchitectConsoleHost {
    cwd: JsString,
    generation: u64,
    prompt_queue: Arc<ArchitectConsolePromptQueue>,
    session: AgentSession,
}

impl ResidentConsoleHost<SessionEntry, AgentSessionEvent> for ArchitectConsoleHost {
    fn get_snapshot(&self) -> ResidentConsoleSnapshot<SessionEntry> {
        create_architect_console_snapshot(
            &self.cwd,
            self.generation,
            Some(&AgentSession::session_id(&self.session)),
            self.session.session_manager().get_branch(None),
        )
    }

    fn enqueue_prompt(&self, text: JsString, id: JsString) -> BoxFuture<'static, Result<(), JsString>> {
        self.prompt_queue.enqueue(text, id);
        Box::pin(async { Ok(()) })
    }

    fn subscribe(&self, listener: Arc<dyn Fn(AgentSessionEvent) + Send + Sync>) -> Unsubscribe {
        let id = self.session.subscribe(Arc::new(move |event: &AgentSessionEvent| {
            listener(event.clone());
        }));
        let session = self.session.clone();
        Box::new(move || session.unsubscribe(id))
    }
}

/// `createArchitectConsoleServer`.
pub fn create_architect_console_server(
    input: CreateArchitectConsoleServerInput,
) -> ResidentConsoleServer<SessionEntry, AgentSessionEvent> {
    ResidentConsoleServer::new(ResidentConsoleServerOptions {
        socket_path: js_format!("{}.architect-console.sock", input.control_db_path),
        service: ResidentConsoleService::Architect,
        host: Arc::new(ArchitectConsoleHost {
            cwd: input.cwd,
            generation: input.generation,
            prompt_queue: input.prompt_queue,
            session: input.session,
        }),
    })
}

/// Shared handle used by the service loop.
pub struct SharedArchitectConsoleServer {
    server: Arc<ResidentConsoleServer<SessionEntry, AgentSessionEvent>>,
}

impl SharedArchitectConsoleServer {
    pub fn new(server: ResidentConsoleServer<SessionEntry, AgentSessionEvent>) -> Self {
        Self {
            server: Arc::new(server),
        }
    }
}

impl ArchitectConsoleServer for SharedArchitectConsoleServer {
    fn start(&self) -> BoxFuture<'static, Result<(), JsString>> {
        let server = Arc::clone(&self.server);
        Box::pin(async move { server.start().await })
    }

    fn close(&self) -> BoxFuture<'static, Result<(), JsString>> {
        let server = Arc::clone(&self.server);
        Box::pin(async move { server.close().await })
    }
}

/// `openArchitectSession`.
pub fn open_architect_session(
    agent_dir: &JsString,
    cwd: &JsString,
    control_db_path: &JsString,
) -> Result<SessionManager, JsString> {
    let prepared = prepare_resident_session_file(agent_dir, ResidentRole::Architect, control_db_path)
        .map_err(|error| error.message())?;
    let session_dir = Some(&prepared.session_dir);
    if std::path::Path::new(&pi_js_compat::js_json::encode_node_utf8(&prepared.session_path)).exists() {
        SessionManager::open(&prepared.session_path, session_dir, Some(cwd))
    } else {
        SessionManager::create(
            cwd,
            session_dir,
            Some(crate::core::session_manager::NewSessionOptions {
                id: Some(ARCHITECT_SESSION_ID.into()),
                ..Default::default()
            }),
        )
    }
    .map_err(|error| error.message().clone())
}

/// `runArchitectService`.
pub async fn run_architect_service() -> Result<(), pi_ai::request::CallbackError> {
    let agent_dir = crate::config::get_agent_dir().map_err(pi_ai::request::CallbackError::from)?;
    // `process.env.HOME ?? process.cwd()`.
    let cwd = match pi_js_compat::process_env::get("HOME") {
        Some(home) => JsString::from(home),
        None => crate::utils::paths::process_cwd().map_err(pi_ai::request::CallbackError::from)?,
    };
    let control_db_path = get_control_db_path_default()?;
    let mut session_manager = open_architect_session(&agent_dir, &cwd, &control_db_path)?;
    let model_registry = ModelRegistry::create(
        crate::core::auth_storage::AuthStorage::create(Some(&crate::utils::paths::node_join(&agent_dir, "auth.json")))
            .map_err(pi_ai::request::CallbackError::from)?,
        Some(crate::utils::paths::node_join(&agent_dir, "models.json")),
    )?;
    let model = model_registry
        .find("openai-codex", "gpt-5.6-sol")
        .ok_or_else(|| JsString::from("Pi Architect requires openai-codex/gpt-5.6-sol"))?;
    let settings_manager = Arc::new(Mutex::new(create_architect_settings_manager()));
    let store = Arc::new(create_architect_multi_agent_store(
        &mut session_manager,
        Some(&control_db_path),
    )?);
    let observer = Arc::new(ArchitectObserver::new(&control_db_path, None)?);

    let mailbox_observer = Arc::clone(&observer);
    let mailbox_options = MultiAgentExtensionOptions {
        on_session_message_sent: Some(Arc::new(move |message, to_session_id| {
            complete_sent_architect_request(
                &*mailbox_observer,
                &SentArchitectMessage {
                    thread_id: message.thread_id.clone(),
                    to_agent_id: message.to_agent_id.clone(),
                    to_session_id: to_session_id.clone(),
                },
            )
            .map_err(HandlerError::new)
        })),
        store: Some(Arc::clone(&store)),
        ..Default::default()
    };
    let mailbox_factory: ExtensionFactory = Arc::new(move |api| {
        let result = agents_mailbox_extension(api, &mailbox_options);
        Box::pin(async move { result })
    });

    let architect_factory: ExtensionFactory = Arc::new(move |api| {
        let result = (|| -> Result<(), HandlerError> {
            register_architect_project_spec_tool(api)?;
            api.on_before_agent_start(|event: BeforeAgentStartEvent, _context: ExtensionContext| {
                Box::pin(async move {
                    Ok(Some(BeforeAgentStartEventResult {
                        message: None,
                        system_prompt: Some(js_format!("{}\n\n{}", event.system_prompt, ARCHITECT_SYSTEM_PROMPT)),
                    }))
                }) as BoxFuture<'static, Result<Option<BeforeAgentStartEventResult>, HandlerError>>
            })?;
            let gate: ToolGateHandle = Arc::new(move |event: ToolCallEvent, _context: ExtensionContext| {
                Box::pin(async move {
                    Ok(
                        block_architect_global_broadcast(event.tool_name.as_str()).map(|result| ToolCallEventResult {
                            block: result.block,
                            input: None,
                            reason: result.reason,
                            terminate: None,
                        }),
                    )
                }) as BoxFuture<'static, Result<Option<ToolCallEventResult>, HandlerError>>
            });
            api.register_tool_gate(gate).map_err(HandlerError::from)
        })();
        Box::pin(async move { result })
    });

    let result = create_agent_session(CreateAgentSessionOptions {
        agent_dir: Some(agent_dir),
        cwd: Some(cwd.clone()),
        disable_runtime_coordination_inbound: true,
        extension_factories: vec![
            InlineExtension::Factory {
                extension_path: Some("<first-party:agents-mailbox>".into()),
                factory: mailbox_factory,
            },
            InlineExtension::Factory {
                extension_path: Some("<first-party:bwrap>".into()),
                factory: bwrap_extension_factory(BwrapExtensionOptions::default()),
            },
            InlineExtension::Factory {
                extension_path: Some("<architect>".into()),
                factory: architect_factory,
            },
        ],
        model: Some(model),
        model_registry: Some(model_registry),
        exclude_tools: Some(
            ARCHITECT_EXCLUDED_TOOL_NAMES
                .iter()
                .map(|name| (*name).to_owned())
                .collect(),
        ),
        multi_agent_runtime_role: Some(MultiAgentRuntimeRole::Observer),
        rules_scope: Some(RulesScope::Architect),
        multi_agent_store: Some(store),
        session_manager: Some(session_manager),
        settings_manager: Some(settings_manager),
        thinking_level: Some(pi_agent_core::harness::types::ThinkingLevel::High),
        ..Default::default()
    })
    .await
    .map_err(|error| error.message)?;
    let session = result.session;
    let abort_controller = pi_js_compat::abort::AbortController::new();
    let prompt_queue = Arc::new(ArchitectConsolePromptQueue::new());
    let console_server = create_architect_console_server(CreateArchitectConsoleServerInput {
        control_db_path,
        cwd,
        generation: std::process::id() as u64,
        prompt_queue: Arc::clone(&prompt_queue),
        session: session.clone(),
    });
    let stop = create_architect_stop_handler(
        abort_controller.clone(),
        Arc::new({
            let session = session.clone();
            move || {
                let session = session.clone();
                Box::pin(async move {
                    let _ = session.abort().await;
                })
            }
        }),
        Arc::new(|code| std::process::exit(code)),
    );
    install_architect_stop_signals(stop);
    let prompt_session = session.clone();
    let prompt = move |content: JsString| {
        let session = prompt_session.clone();
        async move {
            session
                .prompt(&content, PromptOptions::default())
                .await
                .map_err(|error| error.message().clone())
        }
    };
    run_architect_service_loop(
        Some(Arc::new(SharedArchitectConsoleServer::new(console_server))),
        observer,
        prompt,
        prompt_queue,
        abort_controller.signal(),
    )
    .await
    .map_err(Into::into)
}

fn install_architect_stop_signals(stop: impl Fn() + Send + Sync + 'static) {
    let stop = Arc::new(stop);
    tokio::spawn(async move {
        let mut interrupt = match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt()) {
            Ok(signal) => signal,
            Err(_) => return,
        };
        let mut terminate = match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(signal) => signal,
            Err(_) => return,
        };
        tokio::select! {
            _ = interrupt.recv() => {}
            _ = terminate.recv() => {}
        }
        stop();
    });
}
