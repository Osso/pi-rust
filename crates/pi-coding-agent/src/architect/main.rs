//! Port of `packages/coding-agent/src/architect/main.ts`.
//!
//! Agent-session creation and the resident console server are blocked on the agent-session and
//! extension slices; the queue, snapshot, settings, multi-agent store, gating, cycle, and loop
//! contracts are ported here.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use pi_agent_core::harness::types::BeforeToolCallResult;
use pi_ai::request::BoxFuture;
use pi_js_compat::number::parse_number;
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

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
use crate::core::model_runtime::{CreateModelRuntimeOptions, ModelRuntime};
use crate::core::multi_agent_store::{MultiAgentStore, MultiAgentStoreOptions};
use crate::core::resident_console_transport::{
    ResidentConsoleHost, ResidentConsoleServer, ResidentConsoleServerOptions, ResidentConsoleService,
    ResidentConsoleSnapshot, Unsubscribe,
};
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
/// `ask_architect`, `broadcast`, `contact_parent`, then `SUPERVISOR_ONLY_TOOL_NAMES`.
pub const ARCHITECT_EXCLUDED_TOOL_NAMES: [&str; 5] = [
    "ask_architect",
    "broadcast",
    "contact_parent",
    "ask_supervisor",
    "manage_goal",
];

/// One queued console prompt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchitectConsolePrompt {
    pub id: String,
    pub text: String,
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

    pub fn enqueue(&self, text: String, id: String) {
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
    pub async fn wait(&self, signal: &CancellationToken, timeout_ms: u64) {
        if signal.is_cancelled() || !self.is_empty() {
            return;
        }
        tokio::select! {
            () = self.waiters.notified() => {}
            () = signal.cancelled() => {}
            () = tokio::time::sleep(Duration::from_millis(timeout_ms)) => {}
        }
    }
}

/// `createArchitectConsoleSnapshot`.
pub fn create_architect_console_snapshot(
    cwd: &str,
    generation: u64,
    session_id: Option<&str>,
    branch: Vec<SessionEntry>,
) -> ResidentConsoleSnapshot<SessionEntry> {
    ResidentConsoleSnapshot {
        service: ResidentConsoleService::Architect,
        session_id: session_id.unwrap_or(ARCHITECT_SESSION_ID).to_owned(),
        cwd: cwd.to_owned(),
        generation,
        branch,
    }
}

/// `createArchitectSettingsManager`.
pub fn create_architect_settings_manager() -> SettingsManager {
    let mut settings = Settings::new();
    settings.insert(
        "sandboxProfile".to_owned(),
        serde_json::Value::String("read-only".to_owned()),
    );
    SettingsManager::in_memory(settings, SettingsManagerCreateOptions { project_trusted: None })
}

/// `createArchitectMultiAgentStore`.
pub fn create_architect_multi_agent_store(
    session_manager: &mut SessionManager,
    control_db_path: Option<&str>,
) -> Result<MultiAgentStore, String> {
    let control_db_path = control_db_path.map_or_else(get_control_db_path_default, str::to_owned);
    session_manager
        .set_metadata_control_db_path(
            Some(&control_db_path),
            SetMetadataControlDbPathOptions {
                index_message_text: Some(false),
                cwd_override: None,
            },
        )
        .map_err(|error| error.to_string())?;
    if let Some(session_path) = session_manager.get_session_file() {
        archive_session(&control_db_path, session_path).map_err(|error| error.to_string())?;
    }
    MultiAgentStore::from_session_manager(session_manager, MultiAgentStoreOptions::default())
        .map_err(|error| error.to_string())
}

fn get_control_db_path_default() -> String {
    let state_root = crate::config::get_user_state_root();
    get_control_db_path(&state_root)
}

/// `completeSentArchitectRequest`.
pub trait ArchitectRequestCompleter {
    fn complete_request(&self, request_id: i64, sender_session_id: &str) -> Result<(), String>;
}

impl ArchitectRequestCompleter for ArchitectObserver {
    fn complete_request(&self, request_id: i64, sender_session_id: &str) -> Result<(), String> {
        ArchitectObserver::complete_request(self, request_id, sender_session_id).map_err(|error| error.to_string())
    }
}

pub fn complete_sent_architect_request(
    observer: &dyn ArchitectRequestCompleter,
    input: &SentArchitectMessage,
) -> Result<(), String> {
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
    pub thread_id: Option<pi_js_compat::js_json::JsString>,
    pub to_agent_id: pi_js_compat::js_json::JsString,
    pub to_session_id: String,
}

/// `blockArchitectGlobalBroadcast`.
pub fn block_architect_global_broadcast(tool_name: &str) -> Option<BeforeToolCallResult> {
    let reason = match tool_name {
        "channel_post" => "Pi Architect must use direct agent messaging, not channel_post.",
        "list_sessions" => "Pi Architect must use its bounded structured observer snapshot, not list_sessions.",
        "broadcast" => "Pi Architect must use direct agent messaging, not broadcast.",
        _ => return None,
    };
    Some(BeforeToolCallResult {
        block: Some(true),
        reason: Some(reason.to_owned().into()),
    })
}

/// Observer surface used by the cycle.
pub trait ArchitectCycleObserver {
    fn observe(&self) -> Result<Option<crate::architect::observer::ArchitectObservation>, String>;
    fn renew_requests(&self, _request_ids: &[i64]) -> Result<(), String> {
        Ok(())
    }
}

impl ArchitectCycleObserver for ArchitectObserver {
    fn observe(&self) -> Result<Option<crate::architect::observer::ArchitectObservation>, String> {
        ArchitectObserver::observe(self).map_err(|error| error.to_string())
    }

    fn renew_requests(&self, request_ids: &[i64]) -> Result<(), String> {
        ArchitectObserver::renew_requests(self, request_ids).map_err(|error| error.to_string())
    }
}

/// `runArchitectCycle`.
pub async fn run_architect_cycle<O, P, PFut>(observer: Arc<O>, prompt: P) -> Result<(), String>
where
    O: ArchitectCycleObserver + Send + Sync + 'static,
    P: Fn(String) -> PFut,
    PFut: std::future::Future<Output = Result<(), String>>,
{
    let Some(observation) = observer.observe()? else {
        return Ok(());
    };
    let request_ids: Vec<i64> = observation.requests.iter().map(|request| request.id).collect();
    let renew_task = if request_ids.is_empty() {
        None
    } else {
        let observer = Arc::clone(&observer);
        Some(tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(REQUEST_CLAIM_RENEW_INTERVAL_MS));
            interval.tick().await;
            loop {
                interval.tick().await;
                let _ = observer.renew_requests(&request_ids);
            }
        }))
    };
    let result = prompt(build_architect_prompt(&observation)).await;
    if let Some(task) = renew_task {
        task.abort();
    }
    result
}

/// `createArchitectStopHandler`.
pub fn create_architect_stop_handler(
    abort_controller: CancellationToken,
    abort_session: Arc<dyn Fn() -> BoxFuture<'static, ()> + Send + Sync>,
    exit: Arc<dyn Fn(i32) + Send + Sync>,
) -> impl Fn() + Send + Sync {
    let stopping = AtomicBool::new(false);
    move || {
        if stopping.swap(true, Ordering::SeqCst) {
            return;
        }
        abort_controller.cancel();
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
pub async fn wait_for_architect_interval(signal: &CancellationToken) {
    tokio::select! {
        () = tokio::time::sleep(Duration::from_millis(OBSERVER_INTERVAL_MS)) => {}
        () = signal.cancelled() => {}
    }
}

/// Console server surface used by the service loop.
pub trait ArchitectConsoleServer {
    fn start(&self) -> BoxFuture<'static, Result<(), String>>;
    fn close(&self) -> BoxFuture<'static, Result<(), String>>;
}

/// `runArchitectServiceLoop`.
pub async fn run_architect_service_loop<O, P, PFut>(
    console_server: Option<Arc<dyn ArchitectConsoleServer>>,
    observer: Arc<O>,
    prompt: P,
    prompt_queue: Arc<ArchitectConsolePromptQueue>,
    signal: CancellationToken,
) -> Result<(), String>
where
    O: ArchitectCycleObserver + Send + Sync + 'static,
    P: Fn(String) -> PFut,
    PFut: std::future::Future<Output = Result<(), String>>,
{
    if let Some(console_server) = &console_server {
        console_server.start().await?;
    }
    let result = async {
        while !signal.is_cancelled() {
            run_architect_cycle(Arc::clone(&observer), &prompt).await?;
            let mut console_prompt = prompt_queue.take();
            while let Some(prompt_entry) = console_prompt {
                if signal.is_cancelled() {
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
    pub control_db_path: String,
    pub cwd: String,
    pub generation: u64,
    pub prompt_queue: Arc<ArchitectConsolePromptQueue>,
    pub session: AgentSession,
}

struct ArchitectConsoleHost {
    cwd: String,
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

    fn enqueue_prompt(&self, text: String, id: String) -> BoxFuture<'static, Result<(), String>> {
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
        socket_path: std::path::PathBuf::from(format!("{}.architect-console.sock", input.control_db_path)),
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
    fn start(&self) -> BoxFuture<'static, Result<(), String>> {
        let server = Arc::clone(&self.server);
        Box::pin(async move { server.start().await })
    }

    fn close(&self) -> BoxFuture<'static, Result<(), String>> {
        let server = Arc::clone(&self.server);
        Box::pin(async move { server.close().await })
    }
}

/// `runArchitectService`.
pub async fn run_architect_service() -> Result<(), String> {
    let agent_dir = crate::config::get_agent_dir().display().to_string();
    let cwd = pi_js_compat::process_env::get("HOME").unwrap_or_else(|| {
        std::env::current_dir()
            .map(|path| path.display().to_string())
            .unwrap_or_default()
    });
    let session_dir = std::path::Path::new(&agent_dir).join("architect-sessions");
    std::fs::create_dir_all(&session_dir).map_err(|error| error.to_string())?;
    let suffix = format!("_{ARCHITECT_SESSION_ID}.jsonl");
    let mut candidates: Vec<String> = std::fs::read_dir(&session_dir)
        .map_err(|error| error.to_string())?
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            name.ends_with(&suffix).then_some(name)
        })
        .collect();
    candidates.sort();
    let session_dir_text = session_dir.display().to_string();
    let session_path = candidates.last().map_or_else(
        || session_dir.join(format!("{ARCHITECT_SESSION_ID}.jsonl")),
        |name| session_dir.join(name),
    );
    let mut session_manager = if session_path.exists() {
        SessionManager::open(&session_path.display().to_string(), Some(&session_dir_text), Some(&cwd))
    } else {
        SessionManager::create(
            &cwd,
            Some(&session_dir_text),
            Some(crate::core::session_manager::NewSessionOptions {
                id: Some(ARCHITECT_SESSION_ID.to_owned()),
                ..Default::default()
            }),
        )
    }
    .map_err(|error| error.to_string())?;
    let model_runtime = ModelRuntime::create(CreateModelRuntimeOptions {
        auth_path: Some(std::path::Path::new(&agent_dir).join("auth.json").display().to_string()),
        models_path: Some(Some(
            std::path::Path::new(&agent_dir)
                .join("models.json")
                .display()
                .to_string(),
        )),
        ..Default::default()
    })
    .await
    .map_err(|error| error.to_string())?;
    let model = model_runtime
        .get_model("openai-codex", "gpt-5.6-sol")
        .ok_or_else(|| "Pi Architect requires openai-codex/gpt-5.6-sol".to_owned())?;
    let settings_manager = Arc::new(Mutex::new(create_architect_settings_manager()));
    let control_db_path = get_control_db_path(&crate::config::get_user_state_root());
    let store = Arc::new(create_architect_multi_agent_store(
        &mut session_manager,
        Some(&control_db_path),
    )?);
    let observer = Arc::new(ArchitectObserver::new(&control_db_path, None));

    let mailbox_observer = Arc::clone(&observer);
    let mailbox_options = MultiAgentExtensionOptions {
        on_session_message_sent: Some(Arc::new(move |message, to_session_id| {
            complete_sent_architect_request(
                &*mailbox_observer,
                &SentArchitectMessage {
                    thread_id: message.thread_id.clone(),
                    to_agent_id: message.to_agent_id.clone(),
                    to_session_id: to_session_id.to_owned(),
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
        let result = (|| -> Result<(), String> {
            register_architect_project_spec_tool(api)?;
            api.on_before_agent_start(|event: BeforeAgentStartEvent, _context: ExtensionContext| {
                Box::pin(async move {
                    Ok(Some(BeforeAgentStartEventResult {
                        message: None,
                        system_prompt: Some(format!("{}\n\n{ARCHITECT_SYSTEM_PROMPT}", event.system_prompt)),
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
            api.register_tool_gate(gate)
        })();
        Box::pin(async move { result.map_err(HandlerError::new) })
    });

    let result = create_agent_session(CreateAgentSessionOptions {
        agent_dir: Some(agent_dir),
        cwd: Some(cwd.clone()),
        disable_runtime_coordination_inbound: true,
        extension_factories: vec![
            InlineExtension::Factory {
                extension_path: Some("<first-party:agents-mailbox>".to_owned()),
                factory: mailbox_factory,
            },
            InlineExtension::Factory {
                extension_path: Some("<first-party:bwrap>".to_owned()),
                factory: bwrap_extension_factory(BwrapExtensionOptions::default()),
            },
            InlineExtension::Factory {
                extension_path: Some("<architect>".to_owned()),
                factory: architect_factory,
            },
        ],
        model: Some(model),
        model_runtime: Some(model_runtime),
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
    .await?;
    let session = result.session;
    let abort_controller = CancellationToken::new();
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
    let prompt = move |content: String| {
        let session = prompt_session.clone();
        async move {
            session
                .prompt(&content, PromptOptions::default())
                .await
                .map_err(|error| error.message().to_utf8_lossy())
        }
    };
    run_architect_service_loop(
        Some(Arc::new(SharedArchitectConsoleServer::new(console_server))),
        observer,
        prompt,
        prompt_queue,
        abort_controller,
    )
    .await
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
