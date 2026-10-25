//! Shared extension command-context actions for the non-interactive modes.
//!
//! `modes/print-mode.ts` and `modes/rpc/rpc-mode.ts` build the same six runtime
//! callbacks; the interactive mode adds UI callbacks and is ported separately.

use pi_js_compat::js_json::JsString;
use std::sync::{Arc, Mutex};

use crate::core::agent_session::{AgentSession, NavigateTreeOptions as SessionNavigateTreeOptions};
use crate::core::agent_session_runtime::{
    AgentSessionRuntime, ForkPosition, NewSessionOptions as RuntimeNewSessionOptions, ReplacedSessionCallback,
    SessionSetupCallback as RuntimeSessionSetupCallback,
};
use crate::core::extensions::types::{
    ExtensionCommandContextActions, ForkOptions, HandlerError, NavigateTreeOptions, NewSessionOptions, RestartOptions,
    SessionTransitionResult, SwitchSessionOptions, WithSessionCallback,
};
use crate::core::session_manager::SharedSessionManager;

type SharedSession = Arc<Mutex<AgentSession>>;
type SharedRuntime = AgentSessionRuntime;

/// Builds the command-context actions bound by print and RPC mode.
pub fn command_context_actions(session: SharedSession, runtime: SharedRuntime) -> ExtensionCommandContextActions {
    let wait_for_idle_session = Arc::clone(&session);
    let new_session_runtime = runtime.clone();
    let fork_runtime = runtime.clone();
    let navigate_tree_session = Arc::clone(&session);
    let switch_session_runtime = runtime.clone();
    let reload_runtime = runtime.clone();
    let restart_runtime = runtime.clone();
    ExtensionCommandContextActions {
        show_approval_selector: Arc::new(|| Ok(())),
        show_sandbox_selector: Arc::new(|| Ok(())),
        wait_for_idle: Arc::new(move || {
            let session = lock(&wait_for_idle_session).clone();
            Box::pin(async move { session.agent().wait_for_idle().await })
        }),
        new_session: Arc::new(move |options: Option<NewSessionOptions>| {
            let runtime = new_session_runtime.clone();
            Box::pin(async move {
                let options = options.unwrap_or_default();
                let result = runtime
                    .new_session(RuntimeNewSessionOptions {
                        parent_session: options.parent_session,
                        setup: options.setup.map(adapt_session_setup),
                        with_session: options.with_session.map(adapt_with_session),
                    })
                    .await
                    .map_err(HandlerError::from)?;
                Ok(SessionTransitionResult {
                    cancelled: result.cancelled,
                })
            })
        }),
        fork: Arc::new(move |entry_id: JsString, options: Option<ForkOptions>| {
            let runtime = fork_runtime.clone();
            Box::pin(async move {
                let options = options.unwrap_or_default();
                let position = match options.position {
                    Some(crate::core::extensions::types::ForkPosition::At) => ForkPosition::At,
                    Some(crate::core::extensions::types::ForkPosition::Before) | None => ForkPosition::Before,
                };
                let with_session = options.with_session.map(adapt_with_session);
                let result = runtime
                    .fork(&entry_id, position, with_session)
                    .await
                    .map_err(HandlerError::from)?;
                Ok(SessionTransitionResult {
                    cancelled: result.cancelled,
                })
            })
        }),
        navigate_tree: Arc::new(move |target_id: JsString, options: Option<NavigateTreeOptions>| {
            let session = Arc::clone(&navigate_tree_session);
            Box::pin(async move {
                let options = options.unwrap_or_default();
                let session = lock(&session).clone();
                let result = session
                    .navigate_tree(
                        &target_id,
                        SessionNavigateTreeOptions {
                            summarize: options.summarize,
                            custom_instructions: options.custom_instructions,
                            replace_instructions: options.replace_instructions,
                            label: options.label,
                        },
                    )
                    .await
                    .map_err(|error| HandlerError::new(error.message().clone()))?;
                Ok(SessionTransitionResult {
                    cancelled: result.cancelled,
                })
            })
        }),
        switch_session: Arc::new(move |session_path: JsString, options: Option<SwitchSessionOptions>| {
            let runtime = switch_session_runtime.clone();
            Box::pin(async move {
                let options = options.unwrap_or_default();
                let with_session = options.with_session.map(adapt_with_session);
                let result = runtime
                    .switch_session(
                        &session_path,
                        crate::core::agent_session_runtime::SwitchSessionOptions {
                            with_session,
                            ..Default::default()
                        },
                    )
                    .await
                    .map_err(HandlerError::from)?;
                Ok(SessionTransitionResult {
                    cancelled: result.cancelled,
                })
            })
        }),
        reload: Arc::new(move || {
            let runtime = reload_runtime.clone();
            Box::pin(async move { runtime.reload(None).await.map_err(HandlerError::from) })
        }),
        restart: Arc::new(move |options: Option<RestartOptions>| {
            let runtime = restart_runtime.clone();
            Box::pin(async move {
                let (notice, process) = options.map_or((None, false), |options| {
                    (options.notice, options.process.unwrap_or(false))
                });
                let runtime = runtime.clone();
                runtime
                    .restart(notice.as_ref(), process)
                    .await
                    .map_err(HandlerError::from)
            })
        }),
    }
}

pub fn adapt_session_setup(setup: crate::core::extensions::types::SessionSetupCallback) -> RuntimeSessionSetupCallback {
    Arc::new(move |manager: SharedSessionManager| {
        let future = setup(manager);
        Box::pin(async move { future.await.map_err(pi_ai::request::CallbackError::from) })
    })
}

pub fn adapt_with_session(with_session: WithSessionCallback) -> ReplacedSessionCallback {
    Arc::new(move |context| {
        let future = with_session(context);
        Box::pin(async move { future.await.map_err(pi_ai::request::CallbackError::from) })
    })
}

fn lock(session: &SharedSession) -> std::sync::MutexGuard<'_, AgentSession> {
    session.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}
