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
use crate::architect::prompt::build_architect_prompt;
use crate::core::multi_agent_store::{MultiAgentStore, MultiAgentStoreOptions};
use crate::core::resident_console_transport::{ResidentConsoleService, ResidentConsoleSnapshot};
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
    let Some(thread_id) = input.thread_id.as_deref() else {
        return Ok(());
    };
    let Some(suffix) = thread_id.strip_prefix(ARCHITECT_REQUEST_THREAD_PREFIX) else {
        return Ok(());
    };
    let request_id = parse_number(suffix);
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
    pub thread_id: Option<String>,
    pub to_agent_id: String,
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
        reason: Some(reason.to_owned()),
        terminate: None,
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
