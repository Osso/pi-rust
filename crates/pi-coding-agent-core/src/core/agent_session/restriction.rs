//! Frozen AgentSession Codex advisory, tool-free validation and bounded execution state.
use super::*;
use crate::core::provider_restriction_recovery::{
    CodexRestrictionProposal, CodexRestrictionRescopeResult, CodexRestrictionValidationResult,
    RequestCodexRestrictionRescopeOptions, format_codex_restriction_validation, parse_codex_restriction_validation,
    request_codex_restriction_rescope,
};
use pi_agent_core::agent::{AgentMessageIdentity, AgentStateIdentity};

const CONTEXT_ENTRY: &str = "codex_restriction_context";

#[derive(Clone)]
pub(super) struct CodexRestrictionContext {
    user_request: JsString,
    request_id: JsString,
}

#[derive(Default)]
pub(super) struct CodexRestrictionContextCache {
    context: Option<CodexRestrictionContext>,
    session_id: Option<JsString>,
    leaf_id: Option<JsString>,
}

#[derive(Clone)]
struct CodexRestrictionSnapshot {
    session_id: JsString,
    request_id: JsString,
    cwd: JsString,
    goal: Option<JsString>,
    identity: AgentStateIdentity,
}

#[derive(Clone)]
pub(super) struct CodexRestrictionValidation {
    proposal: CodexRestrictionProposal,
    failure: AssistantMessage,
    context: CodexRestrictionContext,
    snapshot: CodexRestrictionSnapshot,
    original_tools: Vec<Arc<dyn AgentTool>>,
    tool_revision: u64,
}

impl AgentSession {
    fn read_stored_codex_restriction_context(&self) -> Option<CodexRestrictionContext> {
        let manager = lock(&self.inner.session_manager);
        for entry in manager.get_branch(None).iter().rev() {
            let value = pi_js_compat::js_json::to_value(entry).expect("session entry serializes");
            let object = value.as_object().expect("session entry object");
            if entry.entry_type() != "custom"
                || object.get("customType").and_then(JsValue::as_string) != Some(&JsString::from(CONTEXT_ENTRY))
            {
                continue;
            }
            let data = object.get("data")?.as_object()?;
            let user_request = data.get("userRequest")?.as_string()?.clone();
            let request_id = data.get("requestId")?.as_string()?.clone();
            return (!request_id.is_empty()).then_some(CodexRestrictionContext {
                user_request,
                request_id,
            });
        }
        None
    }

    fn load_codex_restriction_context(&self) -> Option<CodexRestrictionContext> {
        let session_id = self.session_id();
        let leaf_id = lock(&self.inner.session_manager).get_leaf_id().cloned();
        let stale = {
            let cache = lock(&self.inner.codex_restriction_context);
            cache.session_id.as_ref() != Some(&session_id) || cache.leaf_id != leaf_id
        };
        if stale {
            let context = self.read_stored_codex_restriction_context();
            *lock(&self.inner.codex_restriction_context) = CodexRestrictionContextCache {
                context,
                session_id: Some(session_id),
                leaf_id,
            };
        }
        lock(&self.inner.codex_restriction_context).context.clone()
    }

    pub(super) fn save_codex_restriction_user_request(&self, text: &JsString) -> Result<(), AgentSessionError> {
        self.abort_codex_restriction_advice();
        self.cancel_codex_restriction_validation();
        let context = CodexRestrictionContext {
            user_request: text.clone(),
            request_id: pi_ai::utils::random_uuid::random_uuid()?.into(),
        };
        let session_id = self.session_id();
        {
            let mut cache = lock(&self.inner.codex_restriction_context);
            cache.context = Some(context.clone());
            cache.session_id = Some(session_id);
        }
        let mut manager = lock(&self.inner.session_manager);
        manager.append_custom_entry(
            &CONTEXT_ENTRY.into(),
            Some(pi_js_compat::js_json!({
                "requestId":context.request_id,"userRequest":context.user_request,
            })),
        )?;
        lock(&self.inner.codex_restriction_context).leaf_id = manager.get_leaf_id().cloned();
        Ok(())
    }

    fn read_codex_restriction_identity(&self) -> AgentStateIdentity {
        self.inner.agent.identity_snapshot(|message| match message {
            AgentMessage::Custom(object) => {
                object.get("customType").and_then(JsValue::as_string) != Some(&JsString::from("supervisor-status"))
            }
            _ => true,
        })
    }

    fn can_apply_codex_restriction_advice(
        &self,
        snapshot: &CodexRestrictionSnapshot,
        cancelled: bool,
        response: Option<&AgentMessageIdentity>,
    ) -> Result<bool, AgentSessionError> {
        let dispatch_cancelled = lock(&self.inner.thinking_timeout_dispatch)
            .as_ref()
            .is_some_and(|dispatch| lock(dispatch).cancelled);
        if cancelled || self.inner.disposed.load(Ordering::SeqCst) || dispatch_cancelled {
            return Ok(false);
        }
        if self.session_id() != snapshot.session_id || self.cwd() != &snapshot.cwd {
            return Ok(false);
        }
        let current = self.read_codex_restriction_identity();
        if !snapshot.identity.has_same_model(&current)
            || self
                .load_codex_restriction_context()
                .as_ref()
                .map(|context| &context.request_id)
                != Some(&snapshot.request_id)
        {
            return Ok(false);
        }
        if lock(&self.inner.session_manager).get_session_goal_json()? != snapshot.goal {
            return Ok(false);
        }
        let expected_count = snapshot.identity.message_tokens().len() + usize::from(response.is_some());
        let messages = current.message_tokens();
        let prefix = snapshot.identity.message_tokens();
        if messages.len() != expected_count || &messages[..prefix.len()] != prefix {
            return Ok(false);
        }
        if response.is_some_and(|response| messages.last() != Some(response)) {
            return Ok(false);
        }
        Ok(!self.has_pending_messages() && !self.inner.agent.has_queued_messages())
    }

    pub(super) fn abort_codex_restriction_advice(&self) {
        let controller = lock(&self.inner.codex_restriction_abort).clone();
        if let Some(controller) = controller {
            controller.abort(None);
        }
    }

    /// Replaces the single cancellation owner; unregister only removes that exact owner.
    pub fn register_supervisor_review_cancellation(
        &self,
        handler: Arc<dyn Fn() + Send + Sync>,
    ) -> Arc<dyn Fn() + Send + Sync> {
        *lock(&self.inner.supervisor_review_cancellation_handler) = Some(handler.clone());
        let weak = Arc::downgrade(&self.inner);
        Arc::new(move || {
            if let Some(inner) = weak.upgrade() {
                let mut active = lock(&inner.supervisor_review_cancellation_handler);
                if active.as_ref().is_some_and(|active| Arc::ptr_eq(active, &handler)) {
                    *active = None;
                }
            }
        })
    }

    pub fn cancel_supervisor_review(&self) -> bool {
        let handler = lock(&self.inner.supervisor_review_cancellation_handler).take();
        let Some(handler) = handler else {
            return false;
        };
        handler();
        true
    }

    async fn request_codex_restriction_advice(
        &self,
        message: &AssistantMessage,
        user_request: &JsString,
    ) -> Result<(CodexRestrictionRescopeResult, bool), AgentSessionError> {
        let controller = AbortController::new();
        *lock(&self.inner.codex_restriction_abort) = Some(controller.clone());
        let cancellation = controller.clone();
        let unregister = self.register_supervisor_review_cancellation(Arc::new(move || {
            cancellation.abort(None);
        }));
        let result = async {
            let active_goal = lock(&self.inner.session_manager).get_session_goal_json()?;
            let decision = request_codex_restriction_rescope(&RequestCodexRestrictionRescopeOptions {
                requester: self.inner.supervisor_decision_requester.clone(),
                control_db_path: self.inner.control_db_path.clone().expect("resolved control DB"),
                sender_session_id: self.session_id(),
                cwd: self.cwd().clone(),
                original_user_request: user_request.clone(),
                failure: message.clone(),
                signal: Some(controller.signal()),
                active_goal,
            })
            .await;
            Ok((decision, controller.signal().aborted()))
        }
        .await;
        unregister();
        let mut active = lock(&self.inner.codex_restriction_abort);
        if active.as_ref() == Some(&controller) {
            *active = None;
        }
        result
    }

    async fn display_blocked_codex_restriction(&self, reason: &JsString) -> Result<bool, AgentSessionError> {
        let content = pi_js_compat::js_format!(
            "No validated narrower scope: {}\nNo automatic task execution; original request remains pending.",
            reason
        );
        self.send_custom_message(
            &"supervisor_restriction_recovery".into(),
            CustomMessageContent::Text(content),
            true,
            None,
            false,
            None,
        )
        .await?;
        Ok(false)
    }

    pub(super) fn cancel_codex_restriction_validation(&self) -> Option<CodexRestrictionValidation> {
        let pending = lock(&self.inner.codex_restriction_validation).take();
        if let Some(pending) = pending.as_ref()
            && self.inner.active_tools_revision.load(Ordering::SeqCst) == pending.tool_revision
        {
            self.inner.agent.set_tools(pending.original_tools.clone());
        }
        pending
    }

    async fn start_codex_restriction_validation(
        &self,
        failure: &AssistantMessage,
        context: CodexRestrictionContext,
        proposal: CodexRestrictionProposal,
        snapshot: CodexRestrictionSnapshot,
    ) -> Result<bool, AgentSessionError> {
        let original_tools = self.inner.agent.tools();
        self.inner.agent.set_tools(Vec::new());
        if let Some(dispatch) = lock(&self.inner.thinking_timeout_dispatch).as_ref() {
            lock(dispatch).recovery_used = true;
        }
        let content = format_codex_restriction_validation(&context.user_request, failure, &proposal);
        *lock(&self.inner.codex_restriction_validation) = Some(CodexRestrictionValidation {
            proposal,
            context,
            failure: failure.clone(),
            snapshot,
            original_tools,
            tool_revision: self.inner.active_tools_revision.load(Ordering::SeqCst),
        });
        self.send_custom_message(
            &"supervisor_restriction_validation".into(),
            CustomMessageContent::Text(content),
            true,
            None,
            false,
            None,
        )
        .await?;
        let current = self.read_codex_restriction_identity();
        lock(&self.inner.codex_restriction_validation)
            .as_mut()
            .expect("pending validation")
            .snapshot
            .identity
            .refresh_message_references(&current)?;
        Ok(true)
    }

    async fn finish_codex_restriction_validation(
        &self,
        message: &AssistantMessage,
        identity: &AgentMessageIdentity,
    ) -> Result<bool, AgentSessionError> {
        let Some(pending) = self.cancel_codex_restriction_validation() else {
            return Ok(false);
        };
        if self.inner.active_tools_revision.load(Ordering::SeqCst) != pending.tool_revision {
            return Ok(false);
        }
        if !self.can_apply_codex_restriction_advice(&pending.snapshot, false, Some(identity))? {
            return Ok(false);
        }
        match parse_codex_restriction_validation(message, &pending.proposal, &pending.context.user_request) {
            CodexRestrictionValidationResult::Denied { reason } => {
                self.display_blocked_codex_restriction(&reason).await
            }
            CodexRestrictionValidationResult::Allowed {
                task,
                reason,
                basis_quote,
            } => {
                let failure = pending
                    .failure
                    .error_message
                    .as_ref()
                    .map_or_else(|| JsString::from("undefined"), Clone::clone);
                let content = pi_js_compat::js_format!(
                    "Main validated the narrower task: {}\nReason: {}\nOriginal user text cited: {}\nOriginal provider restriction: {}\nPerform only this bounded task within original constraints. No additional permission or access is granted; do not replay completed effects or attempt the refused portions.",
                    task,
                    reason,
                    basis_quote,
                    failure
                );
                self.send_custom_message(
                    &"supervisor_restriction_recovery".into(),
                    CustomMessageContent::Text(content),
                    true,
                    None,
                    false,
                    None,
                )
                .await?;
                self.inner
                    .codex_restriction_attempt_active
                    .store(true, Ordering::SeqCst);
                Ok(true)
            }
        }
    }

    async fn prepare_codex_restriction_recovery(&self, message: &AssistantMessage) -> Result<bool, AgentSessionError> {
        if !self.is_supervised_main_session() || self.has_pending_messages() || self.inner.agent.has_queued_messages() {
            return Ok(false);
        }
        let Some(context) = self.load_codex_restriction_context() else {
            return self
                .display_blocked_codex_restriction(&"An explicit user request is required for review".into())
                .await;
        };
        let control_db_path = self.inner.control_db_path.as_ref().expect("resolved control DB");
        if !crate::core::session_control_db::claim_codex_restriction_recovery(control_db_path, &context.request_id)
            .map_err(|failure| error(failure.message().clone()))?
        {
            return self
                .display_blocked_codex_restriction(
                    &"Recovery was already used for this request; send a new request".into(),
                )
                .await;
        }
        let snapshot = CodexRestrictionSnapshot {
            session_id: self.session_id(),
            request_id: context.request_id.clone(),
            cwd: self.cwd().clone(),
            goal: lock(&self.inner.session_manager).get_session_goal_json()?,
            identity: self.read_codex_restriction_identity(),
        };
        let (decision, cancelled) = self
            .request_codex_restriction_advice(message, &context.user_request)
            .await?;
        if !self.can_apply_codex_restriction_advice(&snapshot, cancelled, None)? {
            return Ok(false);
        }
        match decision {
            CodexRestrictionRescopeResult::Blocked { reason } => self.display_blocked_codex_restriction(&reason).await,
            CodexRestrictionRescopeResult::Rescope(proposal) => {
                self.start_codex_restriction_validation(message, context, proposal, snapshot)
                    .await
            }
        }
    }

    pub(super) fn codex_restriction_retry_after_agent_end(&self) -> Result<Option<bool>, AgentSessionError> {
        let pending = lock(&self.inner.codex_restriction_validation).clone();
        if let Some(pending) = pending {
            let message = lock(&self.inner.last_assistant_message).clone();
            let Some((message, identity)) = message else {
                return Ok(Some(false));
            };
            let applies = self.can_apply_codex_restriction_advice(&pending.snapshot, false, Some(&identity))?;
            let allowed = matches!(
                parse_codex_restriction_validation(&message, &pending.proposal, &pending.context.user_request),
                CodexRestrictionValidationResult::Allowed { .. }
            );
            return Ok(Some(applies && allowed));
        }
        Ok(self
            .inner
            .codex_restriction_attempt_active
            .load(Ordering::SeqCst)
            .then_some(false))
    }

    pub(super) fn handle_post_agent_run(&self) -> BoxFuture<'_, Result<Option<&'static str>, AgentSessionError>> {
        Box::pin(async move {
            if self.inner.disposed.load(Ordering::SeqCst) {
                return Ok(None);
            }
            let Some((message, identity)) = lock(&self.inner.last_assistant_message).take() else {
                return Ok(None);
            };
            let restriction_attempt = self
                .inner
                .codex_restriction_attempt_active
                .swap(false, Ordering::SeqCst);
            if lock(&self.inner.codex_restriction_validation).is_some() {
                return if self.finish_codex_restriction_validation(&message, &identity).await? {
                    Ok(Some("normal"))
                } else {
                    self.select_post_agent_run_continuation().await
                };
            }
            if pi_ai::utils::retry::is_codex_cybersecurity_restriction(&message) {
                let rescope = self.prepare_codex_restriction_recovery(&message).await?;
                self.finish_exhausted_retry(&message);
                return if rescope {
                    Ok(Some("normal"))
                } else {
                    self.select_post_agent_run_continuation().await
                };
            }
            if restriction_attempt {
                self.finish_exhausted_retry(&message);
                return self.select_post_agent_run_continuation().await;
            }
            if self.prepare_quota_fallback(&message).await? {
                return Ok(Some("normal"));
            }
            if self.is_retryable_error(&message) && self.prepare_retry(&message).await? {
                return Ok(Some("normal"));
            }
            self.finish_exhausted_retry(&message);
            if self.check_compaction(&message, Some(&identity), true).await? {
                return Ok(Some("normal"));
            }
            self.select_post_agent_run_continuation().await
        })
    }

    async fn select_post_agent_run_continuation(&self) -> Result<Option<&'static str>, AgentSessionError> {
        if self.inner.agent.has_queued_messages() {
            return Ok(Some("queued"));
        }
        let ready = self
            .drain_runtime_coordination_messages(DrainCoordinationOptions {
                checkpoint: Some(SteeringCheckpoint::NextModelCall),
                include_next_model_call: false,
                trigger_if_idle: false,
            })
            .await?;
        Ok(ready.then_some("queued"))
    }
}
