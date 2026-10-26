//! Query actors park at Pi tool boundaries while the Claude process remains alive.
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use pi_ai::request::SimpleStreamOptions;
use pi_ai::types::{AssistantMessageEvent, Context, Model, StopReason, Tool};
use pi_ai::utils::event_stream::{AssistantMessageEventStream, EventSender, create_assistant_message_event_stream};
use pi_js_compat::callback_error::CallbackError;
use pi_js_compat::js_json::{self, JsString, JsValue};
use tokio::sync::mpsc;

use super::process::{ClaudeProcess, LaunchOptions, user_message};
use super::stream::{StreamDecoder, string};

type Sender = EventSender<AssistantMessageEvent, pi_ai::types::AssistantMessage>;
type SharedUi = Arc<Mutex<Option<Arc<dyn crate::core::extensions::types::ExtensionUiContext>>>>;
type SharedAccounts = Arc<Mutex<Result<Option<super::account_state::AccountManager>, CallbackError>>>;

pub enum QueryCommand {
    Resume { sender: Sender, context: Context },
    EndTurn { context: Context },
    Shutdown { reason: JsString },
}

#[derive(Default)]
struct QueryObservations {
    session_id: Option<String>,
    active: bool,
    pending_tool_calls: usize,
    prompt_stream: bool,
    shape_snapshot: Option<super::shape_trace::ShapeSnapshot>,
}

struct QueryHandle {
    sender: mpsc::UnboundedSender<QueryCommand>,
    stop: pi_js_compat::abort::AbortController,
    tool_ids: Arc<Mutex<HashSet<String>>>,
    session: JsString,
    settled: tokio::sync::watch::Receiver<bool>,
    observations: Arc<Mutex<QueryObservations>>,
    stale: bool,
}

#[derive(Default)]
struct QueryState {
    next_id: u64,
    primary_query: Option<u64>,
    primary_observations: Option<Arc<Mutex<QueryObservations>>>,
    queries: HashMap<u64, QueryHandle>,
    throttled: HashMap<JsString, PendingProfiles>,
    quota_retry: HashSet<JsString>,
}

struct PendingProfiles {
    prompt_key: JsValue,
    profiles: std::collections::VecDeque<String>,
}

#[cfg(test)]
#[derive(Clone)]
pub(super) enum SteerWriteFault {
    MissingStream,
    Rejected(JsString),
}

#[derive(Clone)]
pub struct BridgeProvider {
    state: Arc<Mutex<QueryState>>,
    pub(super) identity: Arc<()>,
    ownership: Arc<super::ownership::Ownership>,
    pub(super) ui: SharedUi,
    pub(super) mode: Arc<Mutex<Option<crate::core::extensions::types::ExtensionMode>>>,
    pub(super) logs: Result<super::logging::Logs, CallbackError>,
    #[cfg(test)]
    steer_write_fault: Arc<Mutex<Option<SteerWriteFault>>>,
    pub(super) startup_notice: Arc<Mutex<Option<super::startup_notice::StartupNotice>>>,
    pub sessions: Arc<Mutex<Result<super::sessions::SessionStore, CallbackError>>>,
    pub settings: JsValue,
    pub executable: Result<PathBuf, CallbackError>,
    pub ask_name: String,
    pub captures: Arc<Mutex<super::prompt_capture::PromptCaptures>>,
    pub accounts: SharedAccounts,
    pub(super) usage_runtime: super::usage::UsageRuntime,
}

impl BridgeProvider {
    #[cfg(test)]
    pub fn new(settings: JsValue, executable: Result<PathBuf, CallbackError>, ask_name: String) -> Self {
        Self::with_ownership(
            settings,
            executable,
            ask_name,
            Arc::new(super::ownership::Ownership::default()),
        )
    }

    pub(super) fn with_ownership(
        settings: JsValue,
        executable: Result<PathBuf, CallbackError>,
        ask_name: String,
        ownership: Arc<super::ownership::Ownership>,
    ) -> Self {
        let accounts = (|| {
            let Some(names) = super::account_profiles::configured_names(&settings)? else {
                return Ok(None);
            };
            let home = pi_js_compat::node_os::homedir().map_err(|e| CallbackError::new(e.to_string()))?;
            let home = PathBuf::from(home.to_utf8().map_err(|e| CallbackError::new(e.to_string()))?);
            let names = names
                .iter()
                .map(|name| name.to_utf8().map_err(|e| CallbackError::new(e.to_string())))
                .collect::<Result<Vec<_>, _>>()?;
            super::account_state::AccountManager::new(home, names).map(Some)
        })();
        let logs = super::logging::Logs::new();
        let bridge = Self {
            state: Arc::new(Mutex::new(QueryState::default())),
            identity: Arc::new(()),
            ui: Arc::new(Mutex::new(None)),
            mode: Arc::new(Mutex::new(None)),
            startup_notice: Arc::new(Mutex::new(None)),
            logs: logs.clone(),
            #[cfg(test)]
            steer_write_fault: Arc::new(Mutex::new(None)),
            ownership,
            accounts: Arc::new(Mutex::new(accounts)),
            usage_runtime: super::usage::UsageRuntime::default(),
            settings,
            sessions: Arc::new(Mutex::new(super::sessions::SessionStore::from_environment())),
            executable,
            ask_name,
            captures: super::prompt_capture::shared_captures(logs),
        };
        let state = Arc::downgrade(&bridge.state);
        let sessions = Arc::downgrade(&bridge.sessions);
        let rewrite_logs = bridge.logs.clone();
        bridge.ownership.subscribe(
            &bridge.identity,
            Arc::new(move |session, rotate, event| {
                if let (Some(state), Some(sessions)) = (state.upgrade(), sessions.upgrade()) {
                    mark_local_rebuild(&state, &sessions, session, rotate, event, &rewrite_logs)?;
                }
                Ok(())
            }),
        );
        bridge
    }

    #[cfg(test)]
    pub(super) fn inject_steer_failure(&self, error: Option<JsString>) {
        *self.steer_write_fault.lock().unwrap() =
            Some(error.map_or(SteerWriteFault::MissingStream, SteerWriteFault::Rejected));
    }
    pub fn stream(
        &self,
        model: Model,
        mut context: Context,
        options: Option<SimpleStreamOptions>,
    ) -> Result<AssistantMessageEventStream, CallbackError> {
        let ui = self
            .ui
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        let mode = *self.mode.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if let (Some(ui), Some(mode), Some(notice)) = (
            ui,
            mode,
            self.startup_notice
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .as_mut(),
        ) {
            notice.show(mode, ui.as_ref())?;
        }
        if options
            .as_ref()
            .is_some_and(|options| options.stream.cache_retention == Some(pi_ai::types::CacheRetention::None))
        {
            self.logs.as_ref().map_err(|e| e.clone())?.debug(format!(
                "provider: one-off summarizer call (cacheRetention none) routed to isolated summary, msgs={}",
                context.messages.len()
            ))?;
            return Ok(super::summaries::stream(
                self.clone(),
                model,
                context,
                options.unwrap_or_default(),
            ));
        }
        if let Some(tools) = &mut context.tools {
            tools.retain(|tool| tool.name != self.ask_name && tool.name != "web_search");
        }
        let options = options.unwrap_or_default();
        let (sender, stream) = create_assistant_message_event_stream();
        let results = recent_tool_result_ids(&context);
        let active_query = {
            let state = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            state.primary_query.is_some_and(|id| state.queries.contains_key(&id))
        };
        self.logs.as_ref().map_err(|e| e.clone())?.debug(format!(
            "provider: streamClaudeAgentSdk called, activeQuery={}, lastMsgRole={}, isReentrant={}",
            active_query,
            context.messages.last().map_or("undefined", |m| match m {
                pi_ai::types::Message::User(_) => "user",
                pi_ai::types::Message::Assistant(_) => "assistant",
                pi_ai::types::Message::ToolResult(_) => "toolResult",
            }),
            active_query
        ))?;
        let mut state = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let session_key = options.stream.session_id.clone().unwrap_or_else(|| "(none)".into());
        if !state.queries.is_empty() {
            self.logs
                .as_ref()
                .map_err(|e| e.clone())?
                .debug_extract_results(&context)?;
        }
        let previous_primary_shape = state.primary_observations.as_ref().and_then(|observation| {
            observation
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .shape_snapshot
                .clone()
        });
        let stale_ids = state
            .queries
            .iter()
            .filter_map(|(id, q)| (q.stale && q.session == session_key).then_some(*id))
            .collect::<Vec<_>>();
        let rewritten = !stale_ids.is_empty();
        if rewritten {
            self.logs.as_ref().map_err(|e| e.clone())?.debug(
                "provider: history rewritten under a parked query — discarded it, rebuilding from current history",
            )?;
        }
        for id in stale_ids {
            if let Some(query) = state.queries.remove(&id) {
                query.stop.abort(Some(
                    JsValue::String("Context was compacted; this query was discarded.".into()).into(),
                ));
                let _closed = query.sender.send(QueryCommand::Shutdown {
                    reason: "Context was compacted; this query was discarded.".into(),
                });
            }
        }
        let owner = state.queries.values().find(|query| {
            query.session == session_key
                && query
                    .tool_ids
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .iter()
                    .any(|id| results.contains(id))
        });
        if let Some(owner) = owner {
            owner
                .sender
                .send(QueryCommand::Resume { sender, context })
                .map_err(|_| CallbackError::new("Claude query ended before tool result delivery"))?;
            return Ok(stream);
        }
        if matches!(context.messages.last(), Some(pi_ai::types::Message::ToolResult(_)))
            && !state.quota_retry.contains(&session_key)
            && !rewritten
        {
            if state.queries.is_empty() {
                self.sessions
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .as_mut()
                    .map_err(|e| e.clone())?
                    .advance_cursor(&session_key, context.messages.len());
            }
            drop(state);
            self.logs
                .as_ref()
                .map_err(|e| e.clone())?
                .debug("provider: orphaned tool result after abort, emitting end_turn")?;
            let output = pi_ai::types::AssistantMessage::empty(
                &model.api,
                &model.provider,
                &model.id,
                super::account_state::now_seconds()? * 1000.0,
            );
            sender.push(AssistantMessageEvent::Done {
                reason: pi_ai::types::DoneReason::Stop,
                message: output,
            });
            sender.end(None);
            return Ok(stream);
        }
        let is_reentrant = state.primary_query.is_some_and(|id| state.queries.contains_key(&id));
        if is_reentrant && results.is_empty() && matches!(context.messages.last(), Some(pi_ai::types::Message::User(_)))
        {
            let waiting = state
                .primary_query
                .and_then(|id| state.queries.get(&id))
                .map_or(0, |q| {
                    q.observations
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .pending_tool_calls
                });
            self.logs.as_ref().map_err(|e| e.clone())?.debug(format!("provider: active query user-only call treated as reentrant fresh query, waitingHandlers={waiting}, ctx.msgs={}", context.messages.len()))?;
        }
        self.logs.as_ref().map_err(|e| e.clone())?.debug(format!(
            "provider: fresh query setup, isReentrant={}, activeContexts={}",
            is_reentrant,
            state.queries.len()
        ))?;
        let quota_retry = state.quota_retry.remove(&session_key);
        if !rewritten && !quota_retry {
            self.logs.as_ref().map_err(|e| e.clone())?.diag_empty_prompt(
                &context,
                is_reentrant,
                state.queries.len(),
                self.sessions
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .as_ref()
                    .map_err(|e| e.clone())?
                    .debug_shared_session(&session_key),
            )?;
        }
        if (rewritten || quota_retry) && !matches!(context.messages.last(), Some(pi_ai::types::Message::User(_))) {
            self.logs.as_ref().map_err(|e| e.clone())?.debug(format!(
                "provider: continuing the turn after a rewritten history, {} msgs rebuilt",
                context.messages.len()
            ))?;
            context.messages.push(pi_ai::types::Message::User(pi_ai::types::UserMessage {
                role: pi_ai::types::UserRole::Tag,content: pi_ai::types::UserContent::Text("[Your context was compacted. What precedes this is a summary plus the most recent messages, ending with the tool result you were waiting for. Continue the task from there.]".into()),input_source: None,provider_native: None,timestamp: super::account_state::now_seconds()? * 1000.0,
            }));
        }
        let cwd = options
            .stream
            .cwd
            .as_ref()
            .filter(|v| !v.is_empty())
            .ok_or_else(|| CallbackError::new("claude-bridge: stream options.cwd is required"))?;
        let cwd = PathBuf::from(
            cwd.to_utf8()
                .map_err(|_| CallbackError::new("Invalid Claude working directory"))?,
        );
        state.next_id += 1;
        let id = state.next_id;
        if !is_reentrant {
            state.primary_query = Some(id);
        }
        let (commands, receive) = mpsc::unbounded_channel();
        let observations = Arc::new(Mutex::new(QueryObservations {
            shape_snapshot: if is_reentrant {
                None
            } else {
                previous_primary_shape.clone()
            },
            ..Default::default()
        }));
        if !is_reentrant {
            state.primary_observations = Some(observations.clone());
        }
        let (settled, completion) = tokio::sync::watch::channel(false);
        let tool_ids = Arc::new(Mutex::new(HashSet::new()));
        let stop = pi_js_compat::abort::AbortController::new();
        let stop_signal = stop.signal();
        state.queries.insert(
            id,
            QueryHandle {
                sender: commands,
                stop,
                tool_ids: tool_ids.clone(),
                session: session_key,
                settled: completion,
                observations: observations.clone(),
                stale: false,
            },
        );
        let bridge = self.clone();
        drop(state);
        tokio::spawn(async move {
            let mut actor = QueryActor::new(
                &model,
                &context,
                sender,
                (receive, stop_signal),
                tool_ids,
                &bridge,
                observations,
            );
            let key = options.stream.session_id.clone().unwrap_or_else(|| "(none)".into());
            actor.session_key = key.clone();
            actor.is_reentrant = is_reentrant;
            let launch = bridge
                .prepare_launch(
                    &model,
                    &context,
                    &options,
                    cwd.clone(),
                    previous_primary_shape.filter(|_| !is_reentrant),
                )
                .await;
            let auth_profile = launch
                .as_ref()
                .ok()
                .and_then(|launch| launch.cli_debug.as_ref())
                .and_then(|debug| debug.profile.clone());
            let (mut result, preserve_shared_session) = match launch {
                Ok(launch) => (
                    actor.run(&launch, &context, &options).await,
                    launch.preserve_shared_session,
                ),
                Err(error) => {
                    let error = if error.message.starts_with("prompt-capture:") {
                        match bridge.logs.as_ref().map_err(|e| e.clone()).and_then(|logs| {
                            let mut data = pi_js_compat::js_json::JsObject::new();
                            data.insert(
                                "promptChars".into(),
                                JsValue::Number(context.system_prompt.as_ref().map_or(0, JsString::len) as f64),
                            );
                            data.insert(
                                "knownKeys".into(),
                                JsValue::Number(
                                    bridge
                                        .captures
                                        .lock()
                                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                                        .known_count() as f64,
                                ),
                            );
                            data.insert(
                                "reentrantUserQuery".into(),
                                JsValue::Bool(
                                    is_reentrant
                                        && matches!(context.messages.last(), Some(pi_ai::types::Message::User(_)))
                                        && recent_tool_result_ids(&context).is_empty(),
                                ),
                            );
                            data.insert("error".into(), JsValue::String(error.message.clone()));
                            logs.diag("prompt_capture_unresolved", data)
                        }) {
                            Ok(()) => error,
                            Err(log_error) => log_error,
                        }
                    } else {
                        error
                    };
                    (Err(error), false)
                }
            };
            if let Some(profile) = auth_profile {
                let trace = super::auth_trace::record(&profile, "settled", Some("isolated")).and_then(|()| {
                    let error = result.as_ref().err().map(|e| &e.message).or(actor
                        .decoder
                        .output
                        .error_message
                        .as_ref());
                    if let Some(error) = error {
                        super::auth_trace::record_failure(&profile, error)?;
                    }
                    Ok(())
                });
                if let Err(error) = trace {
                    result = Err(error);
                }
            }
            let mut state = bridge.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            if actor.quota_failed {
                state.quota_retry.insert(key.clone());
            }
            if actor.decoder.output.stop_reason != StopReason::Error {
                state.throttled.remove(&key);
            }
            let owned = state.queries.remove(&id).is_some();
            if let Err(error) = actor.log_completion(
                &result,
                owned,
                options.stream.request.signal.as_ref().is_some_and(|s| s.aborted()),
                &super::models::cli_model_id(&model.id, &bridge.settings),
            ) {
                result = Err(error);
            }
            let failed = result.is_err() || actor.decoder.output.stop_reason == StopReason::Aborted;
            let persist = if owned && let Some(native_id) = actor.native_session.take() {
                let mut sessions = bridge
                    .sessions
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                sessions.as_mut().map_err(|e| e.clone()).and_then(|store| {
                    if preserve_shared_session {
                        store.delete_ephemeral(&key, &native_id, &cwd)?;
                    } else {
                        store.complete(&key, native_id, actor.latest_cursor, &cwd)?;
                    }
                    if actor.missed_steer {
                        store.mark_rebuild(&key, false);
                    }
                    if failed {
                        store.mark_rebuild(&key, true);
                    }
                    Ok(())
                })
            } else {
                Ok(())
            };
            let result = result.and(persist);
            match result {
                Err(error) => actor.fail(
                    error,
                    options
                        .stream
                        .request
                        .signal
                        .as_ref()
                        .is_some_and(|signal| signal.aborted()),
                ),

                Ok(()) => actor.finish(),
            };
            {
                let mut observation = actor
                    .observations
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                observation.active = false;
                observation.prompt_stream = false;
                observation.pending_tool_calls = 0;
            }
            settled.send_replace(true);
        });
        Ok(stream)
    }

    pub(super) fn report_leaks(&self, label: &str) -> Result<(), CallbackError> {
        let state = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let (mut contexts, mut calls, mut streams) = (0, 0, 0);
        for query in state.queries.values() {
            let observation = query
                .observations
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if observation.active {
                contexts += 1;
                calls += observation.pending_tool_calls;
                streams += usize::from(observation.prompt_stream);
            }
        }
        if contexts == 0 && calls == 0 && streams == 0 {
            return Ok(());
        }
        self.logs.as_ref().map_err(|e| e.clone())?.debug(format!(
            "WARNING: {label} left state behind — contexts={contexts} pendingToolCalls={calls} promptStreams={streams}"
        ))
    }
    pub fn clear_sessions(&self, event: &str) -> Result<(), CallbackError> {
        let mut sessions = self.sessions.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let store = sessions.as_mut().map_err(|e| e.clone())?;
        let count = store.mirror_count();
        let logs = self.logs.as_ref().map_err(|e| e.clone())?;
        logs.debug(format!(
            "{event}: clearing {count} shared session{}",
            if count == 1 { "" } else { "s" }
        ))?;
        store.clear();
        if self.ownership.release(&self.identity) {
            logs.debug(format!("{event}: clearing ACTIVE_STREAM_SIMPLE_KEY"))?;
        }
        Ok(())
    }

    pub fn mark_rebuild(&self, session: &JsString, rotate: bool, event: &str) -> Result<(), CallbackError> {
        self.ownership.mark_rebuild(session, rotate, event)
    }

    pub async fn shutdown(&self, session: &JsString, reason: &JsString) -> Result<(), CallbackError> {
        let queries = {
            let mut state = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            let ids = state
                .queries
                .iter()
                .filter_map(|(id, q)| (&q.session == session).then_some(*id))
                .collect::<Vec<_>>();
            ids.into_iter()
                .filter_map(|id| state.queries.remove(&id))
                .collect::<Vec<_>>()
        };
        for query in queries {
            query.stop.abort(Some(JsValue::String(reason.clone()).into()));
            let mut settled = query.settled;
            if query
                .sender
                .send(QueryCommand::Shutdown { reason: reason.clone() })
                .is_err()
                && !*settled.borrow()
            {
                return Err(CallbackError::new("Claude query ended before shutdown delivery"));
            }
            while !*settled.borrow_and_update() {
                settled
                    .changed()
                    .await
                    .map_err(|_| CallbackError::new("Claude query ended without settlement"))?;
            }
        }
        Ok(())
    }

    pub async fn end_turn(&self, session: &JsString, context: Context) -> Result<(), CallbackError> {
        let last_assistant = context.messages.iter().rev().find_map(|message| match message {
            pi_ai::types::Message::Assistant(message) => Some(message),
            _ => None,
        });
        if last_assistant.is_none_or(|m| m.stop_reason != StopReason::ToolUse) {
            return Ok(());
        }
        self.logs
            .as_ref()
            .map_err(|e| e.clone())?
            .debug_extract_results(&context)?;
        let results = recent_tool_result_ids(&context);
        let owner = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .queries
            .values()
            .find(|query| {
                &query.session == session
                    && query
                        .tool_ids
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .iter()
                        .any(|id| results.contains(id))
            })
            .map(|q| (q.sender.clone(), q.settled.clone()));
        if let Some((sender, mut settled)) = owner {
            self.logs.as_ref().map_err(|e| e.clone())?.debug(format!(
                "provider: pi run ended on {} tool result(s); closing the CC turn",
                results.len()
            ))?;
            if sender.send(QueryCommand::EndTurn { context }).is_err() && !*settled.borrow() {
                return Err(CallbackError::new(
                    "Claude query ended before terminating tool delivery",
                ));
            }
            while !*settled.borrow_and_update() {
                settled
                    .changed()
                    .await
                    .map_err(|_| CallbackError::new("Claude query ended without settlement"))?;
            }
        }
        Ok(())
    }

    async fn prepare_launch(
        &self,
        model: &Model,
        context: &Context,
        options: &SimpleStreamOptions,
        cwd: PathBuf,
        previous_primary_shape: Option<super::shape_trace::ShapeSnapshot>,
    ) -> Result<LaunchOptions, CallbackError> {
        let mut launch = self
            .prepare_account_launch(model, context, options, cwd.clone())
            .await?;
        if let Some(previous) = previous_primary_shape {
            previous.record_reset()?;
        }
        let mut sessions = self.sessions.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let store = sessions.as_mut().map_err(|e| e.clone())?;
        let key = options.stream.session_id.clone().unwrap_or_else(|| "(none)".into());
        store.verification_logs = Some(self.logs.as_ref().map_err(|e| e.clone())?.clone());
        store.verification_ui = self
            .ui
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        launch.preserve_shared_session = store.preserve_unowned(&key, context);
        launch.resume = store.sync(&key, context, &cwd, &launch.model, true)?;
        self.logs
            .as_ref()
            .map_err(|e| e.clone())?
            .debug_prompt_blocks(context)?;
        self.logs.as_ref().map_err(|e| e.clone())?.debug_fresh_query(
            &launch,
            context,
            self.captures
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .debug_context_file_count(context.system_prompt.as_ref())?,
        )?;
        Ok(launch)
    }

    async fn prepare_account_launch(
        &self,
        model: &Model,
        context: &Context,
        options: &SimpleStreamOptions,
        cwd: PathBuf,
    ) -> Result<LaunchOptions, CallbackError> {
        let key = options.stream.session_id.clone().unwrap_or_else(|| "(none)".into());
        let users = context
            .messages
            .iter()
            .filter(|m| matches!(m, pi_ai::types::Message::User(_)))
            .collect::<Vec<_>>();
        let prompt_key = JsValue::Array(vec![
            JsValue::Number(users.len() as f64),
            js_json::to_value(&users.last()).map_err(|e| CallbackError::new(e.to_string()))?,
        ]);
        let pending = {
            let mut state = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            if state.throttled.get(&key).is_some_and(|p| p.prompt_key != prompt_key) {
                state.throttled.remove(&key);
            }
            state.throttled.get_mut(&key).and_then(|p| p.profiles.pop_front())
        };
        let result =
            self.launch_options_for_profile(model, context, options, cwd.clone(), pending.as_deref(), "provider");
        let mut launch = match result {
            Err(e) if e.message == super::account_state::ACCOUNTS_EXHAUSTED => {
                let signal = options.stream.request.signal.clone().unwrap_or_default();
                let profiles = self.recheck_accounts(&model.id, &signal).await?;
                let mut profiles: std::collections::VecDeque<_> = profiles.into();
                let first = profiles.pop_front();
                self.state
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .throttled
                    .insert(key.clone(), PendingProfiles { prompt_key, profiles });
                let mut launch =
                    self.launch_options_for_profile(model, context, options, cwd, first.as_deref(), "provider")?;
                launch.account_request_started_at = first.map(|_| (self.usage_runtime.now)() / 1000.0);
                launch
            }
            other => other?,
        };
        if pending.is_some() {
            launch.account_request_started_at = Some((self.usage_runtime.now)() / 1000.0);
        }
        launch.account_retries_remaining = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .throttled
            .get(&key)
            .is_some_and(|p| !p.profiles.is_empty());
        Ok(launch)
    }

    pub(super) async fn recheck_accounts(
        &self,
        model: &str,
        signal: &pi_js_compat::abort::AbortSignal,
    ) -> Result<Vec<String>, CallbackError> {
        let home = {
            let accounts = self.accounts.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            accounts
                .as_ref()
                .map_err(|e| e.clone())?
                .as_ref()
                .ok_or_else(|| CallbackError::new(super::account_state::ACCOUNTS_EXHAUSTED))?
                .home()
                .to_path_buf()
        };
        super::usage::UsageService {
            bridge: self.clone(),
            home,
            config_dir: None,
            runtime: self.usage_runtime.clone(),
        }
        .recheck(model, signal)
        .await
        .map_err(CallbackError::new)
    }

    fn mcp_manifest(&self, context: &Context) -> Result<Option<JsValue>, CallbackError> {
        let tools: Vec<_> = context
            .tools
            .as_deref()
            .unwrap_or_default()
            .iter()
            .filter(|tool| tool.name != self.ask_name && tool.name != "web_search")
            .cloned()
            .collect();
        if tools.is_empty() {
            return Ok(None);
        }
        validate_tools(&tools)?;
        let request = |method: &str| JsValue::object([("method", JsValue::String(method.into()))]);
        Ok(Some(JsValue::object([
            ("initializeResult", mcp_result(&request("initialize"), &tools)?),
            ("toolsListResult", mcp_result(&request("tools/list"), &tools)?),
        ])))
    }

    pub fn launch_options(
        &self,
        model: &Model,
        context: &Context,
        options: &SimpleStreamOptions,
        cwd: PathBuf,
        tag: &str,
    ) -> Result<LaunchOptions, CallbackError> {
        self.launch_options_for_profile(model, context, options, cwd, None, tag)
    }

    pub(super) fn launch_options_for_profile(
        &self,
        model: &Model,
        context: &Context,
        options: &SimpleStreamOptions,
        cwd: PathBuf,
        pending: Option<&str>,
        tag: &str,
    ) -> Result<LaunchOptions, CallbackError> {
        let account = {
            let mut accounts = self.accounts.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            match accounts.as_mut().map_err(|e| e.clone())? {
                Some(manager) => Some(if let Some(name) = pending {
                    (
                        name.into(),
                        super::account_profiles::prepare_profile(manager.home(), name)?,
                    )
                } else {
                    manager.prepare_selected(super::account_state::now_seconds()?)?
                }),
                None => None,
            }
        };
        let logs = self.logs.as_ref().map_err(|e| e.clone())?;
        let profile = if let Some((name, _)) = &account {
            let accounts = self.accounts.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            accounts
                .as_ref()
                .map_err(|e| e.clone())?
                .as_ref()
                .map(|manager| super::auth_trace::AuthProfile {
                    home: manager.home().into(),
                    name: name.clone(),
                })
        } else {
            None
        };
        Ok(LaunchOptions {
            append: self
                .captures
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .project(
                    context.system_prompt.as_ref(),
                    if context
                        .tools
                        .as_ref()
                        .is_some_and(|tools| tools.iter().any(|tool| tool.name == "read"))
                    {
                        "mcp"
                    } else {
                        "none"
                    },
                )?,
            mcp_manifest: self.mcp_manifest(context)?,
            cli_debug: Some(logs.cli(tag, profile)?),
            executable: self.executable.clone()?,
            cwd,
            model: super::models::cli_model_id(&model.id, &self.settings),
            provider: self.settings.clone(),
            effort: super::models::effort(model, options.reasoning).map(str::to_owned),
            summary_system_prompt: None,
            summary: false,
            resume: None,
            preserve_shared_session: false,
            claude_dir: account.as_ref().map(|(_, dir)| dir.clone()),
            account_profile: account.map(|(name, _)| name),
            account_request_started_at: None,
            account_retries_remaining: false,
            ask_mode: None,
            isolated: false,
        })
    }
}

struct QueryActor {
    model: Model,
    decoder: StreamDecoder,
    current: Option<Sender>,
    commands: mpsc::UnboundedReceiver<QueryCommand>,
    stop: pi_js_compat::abort::AbortSignal,
    tool_ids: Arc<Mutex<HashSet<String>>>,
    tools: Vec<Tool>,
    pending_calls: indexmap::IndexMap<String, JsValue>,
    pending_results: HashMap<String, JsValue>,
    turn_ended: bool,
    native_session: Option<String>,
    accounts: SharedAccounts,
    rate_limit_rejection: Option<JsValue>,
    latest_cursor: usize,
    quota_failed: bool,
    observations: Arc<Mutex<QueryObservations>>,
    ui: SharedUi,
    warnings: super::rate_limit::WarningState,
    logs: Result<super::logging::Logs, CallbackError>,
    shape: Option<super::shape_trace::SharedShapeTrace>,
    sessions: Arc<Mutex<Result<super::sessions::SessionStore, CallbackError>>>,
    session_key: JsString,
    missed_steer: bool,
    is_reentrant: bool,
    #[cfg(test)]
    steer_write_fault: Arc<Mutex<Option<SteerWriteFault>>>,
}

impl QueryActor {
    fn new(
        model: &Model,
        context: &Context,
        sender: Sender,
        commands: (mpsc::UnboundedReceiver<QueryCommand>, pi_js_compat::abort::AbortSignal),
        tool_ids: Arc<Mutex<HashSet<String>>>,
        bridge: &BridgeProvider,
        observations: Arc<Mutex<QueryObservations>>,
    ) -> Self {
        let tools = context.tools.clone().unwrap_or_default();
        Self {
            model: model.clone(),
            decoder: StreamDecoder::with_tools(model, tools.iter().map(|t| t.name.clone())),
            current: Some(sender),
            commands: commands.0,
            stop: commands.1,
            tool_ids,
            tools,
            pending_calls: indexmap::IndexMap::new(),
            pending_results: HashMap::new(),
            turn_ended: false,
            native_session: None,
            accounts: bridge.accounts.clone(),
            rate_limit_rejection: None,
            latest_cursor: context.messages.len(),
            quota_failed: false,
            observations,
            ui: bridge.ui.clone(),
            warnings: Default::default(),
            logs: bridge.logs.clone(),
            shape: None,
            sessions: bridge.sessions.clone(),
            session_key: "(none)".into(),
            missed_steer: false,
            is_reentrant: false,
            #[cfg(test)]
            steer_write_fault: bridge.steer_write_fault.clone(),
        }
    }

    async fn run(
        &mut self,
        launch: &LaunchOptions,
        context: &Context,
        options: &SimpleStreamOptions,
    ) -> Result<(), CallbackError> {
        validate_tools(&self.tools)?;
        self.observations
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .shape_snapshot = None;
        if launch.provider.get("shapeTrace").and_then(JsValue::as_bool) == Some(true) {
            self.shape = Some(Arc::new(Mutex::new(super::shape_trace::ShapeTrace::new(
                super::logging::agent_dir()?.join("claude-bridge-shapes.jsonl"),
                4 * 1024 * 1024,
                options.stream.session_id.as_ref(),
            )?)));
            self.publish_shape_snapshot(&self.decoder.output);
        }
        let signal = options.stream.request.signal.clone().unwrap_or_default();
        if signal.aborted() {
            self.mark_aborted();
            return Ok(());
        }
        let mut process = ClaudeProcess::spawn_child(launch)?;
        {
            let mut observation = self
                .observations
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            observation.active = true;
            observation.prompt_stream = true;
        }
        let stop = self.stop.clone();
        let outcome = tokio::select! {
            biased;
            _ = signal.wait() => None,
            reason = stop.wait() => Some(Err(CallbackError::new(js_json::to_js_string(&reason.value()).map_err(CallbackError::new)?))),
            result = self.run_query(launch, context, &mut process) => Some(result),
        };
        match outcome {
            None => {
                self.mark_aborted();
                let released = self
                    .release_pending_calls(&"Operation aborted".into(), &mut process)
                    .await;
                process.cancel().await?;
                released
            }
            Some(Err(error)) => {
                process.terminate().await?;
                Err(error)
            }
            Some(Ok(())) => Ok(()),
        }
    }

    async fn run_query(
        &mut self,
        launch: &LaunchOptions,
        context: &Context,
        process: &mut ClaudeProcess,
    ) -> Result<(), CallbackError> {
        process.initialize(launch).await?;
        if let Err(error) = process.write(&user_message(prompt_content(context)?, false)).await {
            self.logs
                .as_ref()
                .map_err(|e| e.clone())?
                .debug_error("provider: initial prompt push rejected:", &error)?;
            return Err(error);
        }
        loop {
            tokio::select! {
                command = self.commands.recv() => match command {
                    Some(QueryCommand::Shutdown { reason }) => {
                        self.tool_ids.lock().unwrap_or_else(std::sync::PoisonError::into_inner).clear();
                        return Err(CallbackError::new(reason));
                    }
                    Some(command) => self.deliver(command,process).await?,
                    None => { return Err(CallbackError::new("Claude query command channel closed")); },
                },
                frame = process.read() => match frame? {
                    None => break,
                    Some(mut frame) => {
                        // The SDK consumes these frames itself (sdk.mjs readMessages); only yielded messages are recorded (index.ts:1858).
                        let sdk_internal = ["control_request", "control_response", "control_cancel_request", "keep_alive", "transcript_mirror"].iter().any(|kind| string(&frame,"type") == *kind);
                        if !sdk_internal && let Some(path)=std::env::var_os("CLAUDE_BRIDGE_RECORD_STREAM").filter(|s|!s.is_empty()) {super::logging::append_line(Path::new(&path),&frame)?;}
                        if let Some(shape)=&self.shape {shape.lock().unwrap_or_else(std::sync::PoisonError::into_inner).record_sdk(&frame)?;}
                        self.settle_account_frame(&mut frame, launch)?;
                        if string(&frame,"type") == "system" && string(&frame,"subtype") == "init" {
                            self.native_session = Some(string(&frame,"session_id").to_utf8().map_err(|e| CallbackError::new(e.to_string()))?);
                            self.observations.lock().unwrap_or_else(std::sync::PoisonError::into_inner).session_id = self.native_session.clone();
                        }
                        if string(&frame,"type") == "control_request" { self.control(&frame,process).await?; }
                        else {
                            self.decode(&frame)?;
                            if string(&frame,"type") == "result" { break; }
                        }
                    }
                }
            }
        }
        self.logs
            .as_ref()
            .map_err(|e| e.clone())?
            .debug(pi_js_compat::js_format!(
                "consumeQuery: for-await loop exited, wasAborted=false, capturedSessionId={}",
                self.native_session
                    .as_deref()
                    .map(super::logging::truncate_session_id)
                    .unwrap_or_else(|| "none".into())
            ))?;
        self.release_pending_calls(&"Query ended".into(), process).await?;

        process.close().await?;
        if self.current.is_some()
            && let Some(shape) = &mut self.shape
        {
            shape
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .record_pi(
                    "pi_final",
                    &js_json::to_value(&self.decoder.output).map_err(|e| CallbackError::new(e.to_string()))?,
                )?;
        }
        Ok(())
    }

    fn settle_account_frame(&mut self, frame: &mut JsValue, launch: &LaunchOptions) -> Result<(), CallbackError> {
        if string(frame, "type") == "rate_limit_event" {
            self.logs
                .as_ref()
                .map_err(|e| e.clone())?
                .debug(pi_js_compat::js_format!(
                    "consumeQuery: rate_limit_event {}",
                    frame
                        .get("rate_limit_info")
                        .map_or_else(
                            || JsString::from("undefined"),
                            |value| JsString::from(js_json::stringify_lossless(value))
                        )
                        .slice(0, 300)
                ))?;
            if let Some(info) = frame.get("rate_limit_info")
                && let Some(message) = self.warnings.observe(info)
            {
                let ui = self
                    .ui
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .clone();
                if let Some(ui) = ui {
                    ui.notify(&message, Some(crate::core::extensions::types::NotifyType::Warning))?;
                }
            }
            if let Some(info) = frame.get("rate_limit_info")
                && string(info, "status") == "rejected"
            {
                self.rate_limit_rejection = Some(info.clone());
            }
            return Ok(());
        }
        if string(frame, "type") != "result" {
            return Ok(());
        }
        self.logs
            .as_ref()
            .map_err(|e| e.clone())?
            .debug_served_context("result", frame, &self.model)?;
        let rejection = self.rate_limit_rejection.take();
        let result_error = super::stream::result_error(frame);
        if let (Some(error), Some(rejection)) = (&result_error, &rejection) {
            let labeled = super::rate_limit::label(rejection, error);
            if let JsValue::Object(fields) = frame {
                fields.insert("subtype".into(), JsValue::String("success".into()));
                fields.insert("is_error".into(), JsValue::Bool(true));
                fields.insert("result".into(), JsValue::String(labeled));
            }
        }
        if let Some(error) = super::stream::result_error(frame) {
            self.logs
                .as_ref()
                .map_err(|e| e.clone())?
                .debug(pi_js_compat::js_format!(
                    "consumeQuery: error result, subtype={}, error={}",
                    string(frame, "subtype"),
                    error
                ))?;
        }
        let Some(name) = launch.account_profile.as_deref() else {
            return Ok(());
        };
        let Some(error) = result_error else {
            if let Some(checked_at) = launch.account_request_started_at {
                let mut accounts = self.accounts.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
                if let Some(manager) = accounts.as_mut().map_err(|e| e.clone())? {
                    manager.recover(name, checked_at)?;
                }
            }
            return Ok(());
        };
        let Some(reset) = quota_rejection(&error, rejection.as_ref()) else {
            return Ok(());
        };
        self.quota_failed = true;
        let mut accounts = self.accounts.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(manager) = accounts.as_mut().map_err(|e| e.clone())?
            && manager
                .reject(name, reset, super::account_state::now_seconds()?)?
                .is_none()
            && !launch.account_retries_remaining
        {
            *frame = JsValue::object([
                ("type", JsValue::String("result".into())),
                ("subtype", JsValue::String("success".into())),
                ("is_error", JsValue::Bool(true)),
                (
                    "result",
                    JsValue::String(super::account_state::ACCOUNTS_EXHAUSTED.into()),
                ),
            ]);
        }
        Ok(())
    }

    fn publish_pending_count(&self) {
        self.observations
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .pending_tool_calls = self.pending_calls.len();
    }
    fn drain_pending_calls(&mut self, reason: &JsString) -> Vec<JsValue> {
        self.observations
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .pending_tool_calls = 0;
        self.pending_results.clear();
        std::mem::take(&mut self.pending_calls)
            .into_values()
            .map(|request| {
                let result = JsValue::object([(
                    "content",
                    JsValue::Array(vec![JsValue::object([
                        ("type", JsValue::String("text".into())),
                        ("text", JsValue::String(reason.clone())),
                    ])]),
                )]);
                mcp_response_frame(&request, Ok(result))
            })
            .collect()
    }

    async fn release_pending_calls(
        &mut self,
        reason: &JsString,
        process: &mut ClaudeProcess,
    ) -> Result<(), CallbackError> {
        let frames = self.drain_pending_calls(reason);
        // SDK resolves all parked callbacks before close, but does not await the
        // resulting transport writes. A blocked peer must not delay teardown.
        tokio::select! {
            biased;
            result = async { for frame in frames {process.write(&frame).await?;} Ok(()) } => result,
            _ = std::future::ready(()) => Ok(()),
        }
    }

    fn mark_aborted(&mut self) {
        self.decoder.output.stop_reason = StopReason::Aborted;
        self.decoder.output.error_message = Some("Operation aborted".into());
        self.tool_ids
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clear();
    }

    fn decode(&mut self, frame: &JsValue) -> Result<(), CallbackError> {
        if self.current.is_none() {
            return Ok(());
        }
        let restream = string(frame, "type") == "stream_event"
            && frame
                .get("event")
                .is_some_and(|event| string(event, "type") == "message_start");
        let replacement = string(frame, "type") == "assistant"
            && frame.get("message").is_some_and(|message| {
                string(message, "model") == "<synthetic>"
                    || (!string(message, "id").is_empty() && string(message, "id") != self.decoder.message_id)
            });
        if self.decoder.stream_open
            && (restream || replacement)
            && let Some(shape) = &mut self.shape
        {
            let discarded = &self.decoder.output.content[self.decoder.block_start..];
            shape
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .record_discarded(
                    &js_json::to_value(discarded).map_err(|e| CallbackError::new(e.to_string()))?,
                    &self.decoder.message_id,
                )?;
        }
        self.logs
            .as_ref()
            .map_err(|e| e.clone())?
            .before_frame(frame, &self.decoder)?;
        let events = self.decoder.decode(frame)?;
        self.publish_shape_snapshot(&self.decoder.output);
        self.logs
            .as_ref()
            .map_err(|e| e.clone())?
            .after_frame(frame, &self.decoder)?;
        for event in events {
            if !self.decoder.parked
                && matches!(
                    event,
                    AssistantMessageEvent::Done { .. } | AssistantMessageEvent::Error { .. }
                )
            {
                continue;
            }
            if let AssistantMessageEvent::ToolCallEnd { tool_call, .. } = &event {
                self.tool_ids
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .insert(tool_call.id.clone());
            }
            if let Some(sender) = &self.current {
                sender.push(event);
            }
        }
        if self.decoder.parked {
            if let Some(shape) = &mut self.shape {
                shape
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .record_pi_with_id(
                        "pi_tool_boundary",
                        &js_json::to_value(&self.decoder.output).map_err(|e| CallbackError::new(e.to_string()))?,
                        &if string(frame, "type") == "assistant" {
                            string(frame.get("message").unwrap_or(&JsValue::Null), "id")
                        } else {
                            self.decoder.message_id.clone()
                        },
                    )?;
            }
            self.current.take();
        }
        Ok(())
    }

    fn publish_shape_snapshot(&self, output: &pi_ai::types::AssistantMessage) {
        if !self.is_reentrant
            && let Some(shape) = &self.shape
        {
            self.observations
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .shape_snapshot = Some(super::shape_trace::ShapeSnapshot::new(
                shape,
                output,
                &self.decoder.message_id,
            ));
        }
    }
    fn log_completion(
        &self,
        result: &Result<(), CallbackError>,
        owned: bool,
        aborted: bool,
        model: &str,
    ) -> Result<(), CallbackError> {
        if !self.is_reentrant && self.shape.is_some() {
            if let Err(error) = result {
                let mut output = self.decoder.output.clone();
                output.stop_reason = if aborted {
                    StopReason::Aborted
                } else {
                    StopReason::Error
                };
                output.error_message = Some(error.message.clone());
                self.publish_shape_snapshot(&output);
            } else {
                self.publish_shape_snapshot(&self.decoder.output);
            }
        }
        if result.is_err()
            && !self
                .observations
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .active
        {
            return Ok(());
        }
        let logs = self.logs.as_ref().map_err(|e| e.clone())?;
        let reason = super::logging::stop_reason(self.decoder.output.stop_reason);
        match result {
            Ok(()) => {
                if aborted {
                    logs.debug(pi_js_compat::js_format!(
                        "consumeQuery: for-await loop exited, wasAborted=true, capturedSessionId={}",
                        self.native_session
                            .as_deref()
                            .map(super::logging::truncate_session_id)
                            .unwrap_or_else(|| "none".into())
                    ))?;
                }
                logs.debug(pi_js_compat::js_format!(
                    "provider: consumeQuery completed, stopReason={}, error={}, aborted={}",
                    reason,
                    self.decoder
                        .output
                        .error_message
                        .as_ref()
                        .cloned()
                        .unwrap_or_else(|| "undefined".into()),
                    if aborted { "true" } else { "false" }
                ))?;
                if !owned {
                    return logs
                        .debug("provider: discarded query completed, leaving session and stream to its replacement");
                }
                if aborted {
                    return logs.debug("provider: abort detected, marked sharedSession needsRebuild + forceRotate");
                }
                if !self.is_reentrant {
                    logs.debug("provider: clearing activeQuery before final stream completion")?;
                }
                if self.current.is_some() {
                    logs.debug_final_stream(&self.decoder.output)?;
                }
            }
            Err(error) => {
                logs.debug_error(
                    pi_js_compat::js_format!(
                        "provider: query error, model={}, aborted={}, error=",
                        model,
                        if aborted { "true" } else { "false" }
                    ),
                    error,
                )?;
                if !owned {
                    return logs.debug(
                        "provider: discarded query ended in error, leaving session and stream to its replacement",
                    );
                }
                if !self.is_reentrant {
                    logs.debug("provider: clearing activeQuery before error stream completion")?;
                }
            }
        }
        Ok(())
    }
    fn finish(&mut self) {
        if let Some(sender) = self.current.take() {
            for event in self.decoder.finish_events() {
                sender.push(event);
            }
            sender.end(None);
        }
    }

    fn fail(&mut self, error: CallbackError, aborted: bool) {
        self.decoder.output.stop_reason = if aborted {
            StopReason::Aborted
        } else {
            StopReason::Error
        };
        self.decoder.output.error_message = Some(error.message);
        self.finish();
    }

    async fn deliver(&mut self, command: QueryCommand, process: &mut ClaudeProcess) -> Result<(), CallbackError> {
        match command {
            QueryCommand::Shutdown { reason } => return Err(CallbackError::new(reason)),
            QueryCommand::EndTurn { context } => {
                self.turn_ended = true;
                self.latest_cursor += 1 + recent_tool_result_ids(&context).len();
                self.accept_results(&context, process).await?;
            }
            QueryCommand::Resume { sender, context } => {
                if self.current.is_some() {
                    self.logs.as_ref().map_err(|e| e.clone())?.debug(format!("WARNING: currentPiStream overwritten before terminal event (tool-result); activeQuery={} pendingHandlers={}", self.observations.lock().unwrap_or_else(std::sync::PoisonError::into_inner).active, self.pending_calls.len()))?;
                }
                if let Some(shape) = &mut self.shape {
                    shape
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .record_pi_with_id(
                            "turn_reset",
                            &js_json::to_value(&self.decoder.output).map_err(|e| CallbackError::new(e.to_string()))?,
                            &self.decoder.message_id,
                        )?;
                }
                self.current = Some(sender);
                self.decoder = StreamDecoder::with_tools(&self.model, self.tools.iter().map(|t| t.name.clone()));
                self.publish_shape_snapshot(&self.decoder.output);
                if matches!(context.messages.last(), Some(pi_ai::types::Message::User(_))) {
                    self.logs
                        .as_ref()
                        .map_err(|e| e.clone())?
                        .debug_prompt_blocks(&context)?;
                    self.deliver_steer(prompt_content(&context)?, process).await?;
                }
                self.latest_cursor = self.latest_cursor.max(context.messages.len());
                self.accept_results(&context, process).await?;
            }
        }
        Ok(())
    }

    fn mark_missed_steer(&mut self, text: &JsString) -> Result<(), CallbackError> {
        self.missed_steer = true;
        self.sessions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .as_mut()
            .map_err(|e| e.clone())?
            .mark_rebuild(&self.session_key, false);
        self.logs
            .as_ref()
            .map_err(|e| e.clone())?
            .debug(pi_js_compat::js_format!(
                "provider: steer never reached CC, marked query for rebuild: {}",
                text.slice(0, 60)
            ))
    }
    async fn write_steer(&self, content: JsValue, process: &mut ClaudeProcess) -> Result<(), CallbackError> {
        #[cfg(test)]
        if let Some(SteerWriteFault::Rejected(message)) = self
            .steer_write_fault
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
        {
            return Err(CallbackError::new(message));
        }
        process.write(&user_message(content, true)).await
    }
    async fn deliver_steer(&mut self, content: JsValue, process: &mut ClaudeProcess) -> Result<(), CallbackError> {
        let text = content
            .as_array()
            .map(|blocks| {
                JsString::join(
                    blocks.iter().map(|b| {
                        if string(b, "type") == "text" {
                            string(b, "text")
                        } else {
                            "[image]".into()
                        }
                    }),
                    "\n",
                )
            })
            .unwrap_or_else(|| content.as_string().cloned().unwrap_or_default());
        let live = self
            .observations
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .prompt_stream;
        #[cfg(test)]
        let live = live
            && !matches!(
                self.steer_write_fault
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .as_ref(),
                Some(SteerWriteFault::MissingStream)
            );
        let logs = self.logs.as_ref().map_err(|e| e.clone())?;
        if !live {
            logs.debug(pi_js_compat::js_format!(
                "WARNING: steer with no prompt stream, dropping: {}",
                text.slice(0, 60)
            ))?;
            self.mark_missed_steer(&text)?;
        } else if let Err(error) = self.write_steer(content, process).await {
            logs.debug_error("provider: steer push rejected, delivering tool result anyway:", &error)?;
            self.mark_missed_steer(&text)?;
        } else {
            logs.debug(pi_js_compat::js_format!(
                "provider: steer written to CC stdin before tool result: {}",
                text.slice(0, 60)
            ))?;
        }
        Ok(())
    }
    async fn accept_results(&mut self, context: &Context, process: &mut ClaudeProcess) -> Result<(), CallbackError> {
        self.logs.as_ref().map_err(|e| e.clone())?.debug_deliver_results(
            context,
            &self.pending_calls,
            &self.pending_results,
        )?;
        for message in context
            .messages
            .iter()
            .rev()
            .take_while(|m| !matches!(m, pi_ai::types::Message::Assistant(_)))
        {
            if let pi_ai::types::Message::ToolResult(result) = message {
                let value = JsValue::object([
                    (
                        "content",
                        super::convert::tool_result_to_mcp_content(
                            &js_json::to_value(&result.content).map_err(|e| CallbackError::new(e.to_string()))?,
                        ),
                    ),
                    ("isError", JsValue::Bool(result.is_error)),
                ]);
                self.pending_results.insert(result.tool_call_id.clone(), value);
            }
        }
        self.flush_results(process).await
    }

    async fn flush_results(&mut self, process: &mut ClaudeProcess) -> Result<(), CallbackError> {
        let ready: Vec<_> = self
            .pending_calls
            .keys()
            .filter(|id| self.pending_results.contains_key(*id))
            .cloned()
            .collect();
        for id in ready {
            if let (Some(request), Some(result)) =
                (self.pending_calls.shift_remove(&id), self.pending_results.remove(&id))
            {
                self.publish_pending_count();
                self.send_mcp_response(&request, Ok(result), process).await?;
            }
        }
        Ok(())
    }

    async fn control(&mut self, frame: &JsValue, process: &mut ClaudeProcess) -> Result<(), CallbackError> {
        let request = frame.get("request").unwrap_or(&JsValue::Null);
        match string(request, "subtype")
            .to_utf8()
            .map_err(|_| CallbackError::new("Invalid Claude control subtype"))?
            .as_str()
        {
            "mcp_message" => {
                let message = request.get("message").unwrap_or(&JsValue::Null);
                if string(message, "method") == "tools/call" {
                    let params = message.get("params").unwrap_or(&JsValue::Null);
                    let name = string(params, "name");
                    let call_id = params
                        .get("_meta")
                        .and_then(|v| v.get("claudecode/toolUseId"))
                        .and_then(JsValue::as_string);
                    if !self.tools.iter().any(|t| name == t.name.as_str()) {
                        return self
                            .send_mcp_response(frame, Err(pi_js_compat::js_format!("Unknown tool: {}", name)), process)
                            .await;
                    }
                    let Some(call_id) = call_id else {
                        return self.send_mcp_response(frame, Err(pi_js_compat::js_format!("{}: tools/call is missing _meta[\"claudecode/toolUseId\"] — cannot pair the result with its tool call",name)),process).await;
                    };
                    let id = call_id
                        .to_utf8()
                        .map_err(|_| CallbackError::new("Invalid Claude tool-use id"))?;
                    self.logs
                        .as_ref()
                        .map_err(|e| e.clone())?
                        .debug(pi_js_compat::js_format!(
                            "mcp handler: {} [{}] → {}",
                            name,
                            call_id,
                            if self.pending_results.contains_key(&id) {
                                format!("resolved from queue ({} remaining)", self.pending_results.len() - 1)
                            } else {
                                "waiting".into()
                            }
                        ))?;
                    self.pending_calls.insert(id, frame.clone());
                    self.publish_pending_count();
                    self.flush_results(process).await?;
                } else {
                    self.send_mcp_response(frame, Ok(mcp_result(message, &self.tools)?), process)
                        .await?;
                }
            }
            "hook_callback" => {
                let result = if self.turn_ended {
                    JsValue::object([("continue", JsValue::Bool(false))])
                } else {
                    JsValue::Object(Default::default())
                };
                process.write(&control_response(frame, result)).await?;
            }
            _ => {
                process
                    .write(&control_response(frame, JsValue::Object(Default::default())))
                    .await?;
            }
        }
        Ok(())
    }

    async fn send_mcp_response(
        &self,
        request: &JsValue,
        result: Result<JsValue, JsString>,
        process: &mut ClaudeProcess,
    ) -> Result<(), CallbackError> {
        process.write(&mcp_response_frame(request, result)).await
    }
}

fn mcp_response_frame(request: &JsValue, result: Result<JsValue, JsString>) -> JsValue {
    let rpc_id = request
        .get("request")
        .and_then(|v| v.get("message"))
        .and_then(|v| v.get("id"))
        .cloned()
        .unwrap_or(JsValue::Number(0.0));
    let response = match result {
        Ok(result) => JsValue::object([
            ("jsonrpc", JsValue::String("2.0".into())),
            ("id", rpc_id),
            ("result", result),
        ]),
        Err(error) => JsValue::object([
            ("jsonrpc", JsValue::String("2.0".into())),
            ("id", rpc_id),
            (
                "error",
                JsValue::object([("code", JsValue::Number(-32603.0)), ("message", JsValue::String(error))]),
            ),
        ]),
    };
    control_response(request, JsValue::object([("mcp_response", response)]))
}

fn mark_local_rebuild(
    state: &Arc<Mutex<QueryState>>,
    sessions: &Arc<Mutex<Result<super::sessions::SessionStore, CallbackError>>>,
    session: &JsString,
    rotate: bool,
    event: &str,
    logs: &Result<super::logging::Logs, CallbackError>,
) -> Result<(), CallbackError> {
    let mut state = state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut sessions = sessions.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let store = sessions.as_mut().map_err(|e| e.clone())?;
    let message = store.mirror_id(session).map_or_else(
        || pi_js_compat::js_format!("{}: history rewritten, no session to mark yet", event),
        |id| {
            pi_js_compat::js_format!(
                "{}: marking needsRebuild on session {}",
                event,
                super::logging::truncate_session_id(id)
            )
        },
    );
    let mut active = false;
    for query in state.queries.values_mut().filter(|q| &q.session == session) {
        query.stale = true;
        active = true;
        if let Some(id) = query
            .observations
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .session_id
            .clone()
        {
            store.seed_live_session(session, id);
        }
    }
    store.mark_rebuild(session, rotate || active);
    logs.as_ref().map_err(|e| e.clone())?.debug(message)
}

fn recent_tool_result_ids(context: &Context) -> HashSet<String> {
    context
        .messages
        .iter()
        .rev()
        .take_while(|m| !matches!(m, pi_ai::types::Message::Assistant(_)))
        .filter_map(|message| match message {
            pi_ai::types::Message::ToolResult(result) => Some(result.tool_call_id.clone()),
            _ => None,
        })
        .collect()
}

fn control_response(request: &JsValue, response: JsValue) -> JsValue {
    JsValue::object([
        ("type", JsValue::String("control_response".into())),
        (
            "response",
            JsValue::object([
                ("subtype", JsValue::String("success".into())),
                ("request_id", JsValue::String(string(request, "request_id"))),
                ("response", response),
            ]),
        ),
    ])
}

pub(super) fn quota_rejection(error: &JsString, rejection: Option<&JsValue>) -> Option<Option<f64>> {
    if error
        == "Your organization has disabled Claude subscription access for Claude Code · Use an Anthropic API key instead, or ask your admin to enable access"
    {
        return Some(None);
    }
    let info = rejection?;
    let window = string(info, "rateLimitType");
    let subscription = [
        "five_hour",
        "seven_day",
        "seven_day_opus",
        "seven_day_sonnet",
        "seven_day_overage_included",
    ]
    .iter()
    .any(|name| window == *name);
    if subscription && string(info, "errorCode") != "credits_required" {
        Some(info.get("resetsAt").and_then(JsValue::as_f64))
    } else {
        None
    }
}

fn mcp_result(message: &JsValue, tools: &[Tool]) -> Result<JsValue, CallbackError> {
    if string(message, "method") == "initialize" {
        return js_json::parse_lossless(r#"{"protocolVersion":"2025-11-25","capabilities":{"tools":{}},"serverInfo":{"name":"custom-tools","version":"1.0.0"}}"#).map_err(|e| CallbackError::new(e.to_string()));
    }
    if string(message, "method") == "tools/list" {
        let tools = tools
            .iter()
            .map(|tool| {
                Ok(JsValue::object([
                    ("name", JsValue::String(tool.name.as_str().into())),
                    ("description", JsValue::String(tool.description.clone())),
                    (
                        "inputSchema",
                        js_json::to_value(&tool.parameters).map_err(|e| CallbackError::new(e.to_string()))?,
                    ),
                ]))
            })
            .collect::<Result<Vec<_>, CallbackError>>()?;
        return Ok(JsValue::object([("tools", JsValue::Array(tools))]));
    }
    Ok(JsValue::Object(Default::default()))
}

fn validate_tools(tools: &[Tool]) -> Result<(), CallbackError> {
    for tool in tools {
        if tool.parameters.get("type").and_then(serde_json::Value::as_str) != Some("object") {
            return Err(CallbackError::new(format!(
                "{}: MCP tool parameters must be an object schema, got {}",
                tool.name, tool.parameters
            )));
        }
    }
    Ok(())
}

pub fn prompt_content(context: &Context) -> Result<JsValue, CallbackError> {
    Ok(super::prompt::extract(context)?.into_content())
}

pub fn executable_path(settings: &JsValue, extension_path: &Path) -> Result<PathBuf, CallbackError> {
    if let Some(path) = settings.get("pathToClaudeCodeExecutable").and_then(JsValue::as_string) {
        return Ok(PathBuf::from(
            path.to_utf8()
                .map_err(|_| CallbackError::new("Invalid Claude executable path"))?,
        ));
    }
    let platform = if cfg!(target_os = "linux") {
        "linux"
    } else if cfg!(target_os = "macos") {
        "darwin"
    } else {
        "win32"
    };
    let arch = if cfg!(target_arch = "aarch64") { "arm64" } else { "x64" };
    for ancestor in extension_path.ancestors().skip(1) {
        let binary = ancestor.join(format!(
            "node_modules/@anthropic-ai/claude-agent-sdk-{platform}-{arch}/claude"
        ));
        if binary.is_file() {
            return Ok(binary);
        }
    }
    Err(CallbackError::new(
        "Could not find Claude Code executable. Set provider.pathToClaudeCodeExecutable in claude-bridge.json.",
    ))
}

#[cfg(test)]
mod teardown_tests {
    use super::*;
    #[test]
    fn claude_bridge_rewrite_events_fan_out_the_source_labels_and_mirror_branches() {
        let dir = tempfile::tempdir().unwrap();
        let owner = super::super::ownership::Ownership::default();
        let identity = Arc::new(());
        let state = Arc::new(Mutex::new(QueryState::default()));
        let sessions = Arc::new(Mutex::new(Ok(super::super::sessions::SessionStore::at(
            dir.path().join("cc"),
            dir.path().join("agent"),
        ))));
        let mut logs = super::super::logging::Logs::new().unwrap();
        logs.enabled = true;
        logs.debug("rewrite probe").unwrap();
        let hook_state = state.clone();
        let hook_sessions = sessions.clone();
        let hook_logs = Ok(logs.clone());
        owner.subscribe(
            &identity,
            Arc::new(move |key, rotate, event| {
                mark_local_rebuild(&hook_state, &hook_sessions, key, rotate, event, &hook_logs)
            }),
        );
        let key = JsString::from("rewrite-log");
        owner.mark_rebuild(&key, false, "session_compact").unwrap();
        sessions
            .lock()
            .unwrap()
            .as_mut()
            .unwrap()
            .record(&key, "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa".into(), 2, dir.path())
            .unwrap();
        owner.mark_rebuild(&key, false, "session_tree").unwrap();
        let text = std::fs::read_to_string(super::super::logging::debug_path().unwrap()).unwrap();
        let lines = text
            .lines()
            .filter_map(|line| {
                line.split_once(&format!("[{}] ", logs.module_id))
                    .map(|(_, message)| message)
            })
            .filter(|line| *line != "rewrite probe")
            .collect::<Vec<_>>();
        assert_eq!(
            lines,
            vec![
                "session_compact: history rewritten, no session to mark yet",
                "session_tree: marking needsRebuild on session aaaaaaaa"
            ]
        );
    }
    #[test]
    fn claude_bridge_teardown_resolves_all_mcp_calls_with_plain_reason_and_clears_queued_results() {
        let model = pi_ai::providers::models_data::builtin_model("anthropic", "claude-opus-5-5").unwrap();
        let context = Context {
            system_prompt: None,
            messages: Vec::new(),
            tools: None,
        };
        let bridge = BridgeProvider::new(JsValue::Null, Err(CallbackError::new("unused")), "AskClaude".into());
        let (sender, _) = pi_ai::utils::event_stream::create_assistant_message_event_stream();
        let (_, commands) = mpsc::unbounded_channel();
        let stop = pi_js_compat::abort::AbortController::new();
        let mut actor = QueryActor::new(
            &model,
            &context,
            sender,
            (commands, stop.signal()),
            Arc::new(Mutex::new(HashSet::new())),
            &bridge,
            Arc::new(Mutex::new(QueryObservations::default())),
        );
        for (tool, id) in [("first", 1.0), ("second", 2.0)] {
            let frame = JsValue::object([
                ("request_id", JsValue::String(tool.into())),
                (
                    "request",
                    JsValue::object([("message", JsValue::object([("id", JsValue::Number(id))]))]),
                ),
            ]);
            actor.pending_calls.insert(tool.into(), frame);
            actor.pending_results.insert(tool.into(), JsValue::Null);
        }
        let frames = actor.drain_pending_calls(&"Operation aborted".into());
        assert_eq!(frames,js_json::parse_lossless(r#"[{"type":"control_response","response":{"subtype":"success","request_id":"first","response":{"mcp_response":{"jsonrpc":"2.0","id":1,"result":{"content":[{"type":"text","text":"Operation aborted"}]}}}}},{"type":"control_response","response":{"subtype":"success","request_id":"second","response":{"mcp_response":{"jsonrpc":"2.0","id":2,"result":{"content":[{"type":"text","text":"Operation aborted"}]}}}}}]"#).unwrap().as_array().unwrap().to_vec());
        assert!(actor.pending_calls.is_empty());
        assert!(actor.pending_results.is_empty());
    }
}
