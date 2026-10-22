//! Shared extension command-context actions for the non-interactive modes.
//!
//! `modes/print-mode.ts` and `modes/rpc/rpc-mode.ts` build the same six runtime
//! callbacks; the interactive mode adds UI callbacks and is ported separately.

use std::sync::{Arc, Mutex};

use tokio::sync::Mutex as AsyncMutex;

use crate::core::agent_session::{AgentSession, NavigateTreeOptions as SessionNavigateTreeOptions};
use crate::core::agent_session_runtime::{
    AgentSessionRuntime, ForkPosition, NewSessionOptions as RuntimeNewSessionOptions, ReplacedSessionCallback,
    SessionSetupCallback as RuntimeSessionSetupCallback,
};
use crate::core::extensions::types::{
    ExtensionCommandContextActions, ForkOptions, HandlerError, NavigateTreeOptions, NewSessionOptions, RestartOptions,
    SessionTransitionResult, SwitchSessionOptions, WithSessionCallback,
};
use crate::core::permissions::presets::{SandboxProfileName, SandboxProfileScope};
use crate::core::session_manager::SessionManager;

type SharedSession = Arc<Mutex<AgentSession>>;
type SharedRuntime = Arc<AsyncMutex<AgentSessionRuntime>>;

/// Builds the command-context actions bound by print and RPC mode.
pub fn command_context_actions(session: SharedSession, runtime: SharedRuntime) -> ExtensionCommandContextActions {
    let wait_for_idle_session = Arc::clone(&session);
    let new_session_runtime = Arc::clone(&runtime);
    let fork_runtime = Arc::clone(&runtime);
    let navigate_tree_session = Arc::clone(&session);
    let switch_session_runtime = Arc::clone(&runtime);
    let reload_session = Arc::clone(&session);
    let restart_runtime = Arc::clone(&runtime);
    let set_sandbox_profile_session = Arc::clone(&session);
    ExtensionCommandContextActions {
        show_approval_selector: Arc::new(|| Ok(())),
        show_sandbox_selector: Arc::new(|| Ok(())),
        set_sandbox_profile: Arc::new(move |profile: Option<SandboxProfileName>, scope: SandboxProfileScope| {
            lock(&set_sandbox_profile_session)
                .set_sandbox_profile(profile, scope)
                .map_err(|error| HandlerError::new(error.to_string()))
        }),
        wait_for_idle: Arc::new(move || {
            let session = lock(&wait_for_idle_session).clone();
            Box::pin(async move { session.agent().wait_for_idle().await })
        }),
        new_session: Arc::new(move |options: Option<NewSessionOptions>| {
            let runtime = Arc::clone(&new_session_runtime);
            Box::pin(async move {
                let options = options.unwrap_or_default();
                let errors = CallbackErrors::default();
                let mut runtime = runtime.lock().await;
                let result = runtime
                    .new_session(RuntimeNewSessionOptions {
                        parent_session: options.parent_session,
                        setup: options.setup.map(|setup| errors.adapt_setup(setup)),
                        with_session: options
                            .with_session
                            .map(|with_session| errors.adapt_with_session(with_session)),
                    })
                    .await
                    .map_err(HandlerError::new)?;
                errors.take()?;
                Ok(SessionTransitionResult {
                    cancelled: result.cancelled,
                })
            })
        }),
        fork: Arc::new(move |entry_id: String, options: Option<ForkOptions>| {
            let runtime = Arc::clone(&fork_runtime);
            Box::pin(async move {
                let options = options.unwrap_or_default();
                let position = match options.position {
                    Some(crate::core::extensions::types::ForkPosition::At) => ForkPosition::At,
                    Some(crate::core::extensions::types::ForkPosition::Before) | None => ForkPosition::Before,
                };
                let errors = CallbackErrors::default();
                let with_session = options
                    .with_session
                    .map(|with_session| errors.adapt_with_session(with_session));
                let mut runtime = runtime.lock().await;
                let result = runtime
                    .fork(&entry_id, position, with_session)
                    .await
                    .map_err(HandlerError::new)?;
                errors.take()?;
                Ok(SessionTransitionResult {
                    cancelled: result.cancelled,
                })
            })
        }),
        navigate_tree: Arc::new(move |target_id: String, options: Option<NavigateTreeOptions>| {
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
                    .map_err(|error| HandlerError::new(pi_js_compat::js_json::encode_node_utf8(error.message())))?;
                Ok(SessionTransitionResult {
                    cancelled: result.cancelled,
                })
            })
        }),
        switch_session: Arc::new(move |session_path: String, options: Option<SwitchSessionOptions>| {
            let runtime = Arc::clone(&switch_session_runtime);
            Box::pin(async move {
                let options = options.unwrap_or_default();
                let errors = CallbackErrors::default();
                let with_session = options
                    .with_session
                    .map(|with_session| errors.adapt_with_session(with_session));
                let mut runtime = runtime.lock().await;
                let result = runtime
                    .switch_session(&session_path, None, with_session)
                    .await
                    .map_err(HandlerError::new)?;
                errors.take()?;
                Ok(SessionTransitionResult {
                    cancelled: result.cancelled,
                })
            })
        }),
        reload: Arc::new(move || {
            let session = Arc::clone(&reload_session);
            Box::pin(async move {
                let session = lock(&session).clone();
                if let Err(error) = session.reload(None).await {
                    eprintln!("{}", pi_js_compat::js_json::encode_node_utf8(error.message()));
                }
            })
        }),
        restart: Arc::new(move |options: Option<RestartOptions>| {
            let runtime = Arc::clone(&restart_runtime);
            Box::pin(async move {
                let (notice, process) = options.map_or((None, false), |options| {
                    (options.notice, options.process.unwrap_or(false))
                });
                let mut runtime = runtime.lock().await;
                runtime
                    .restart(notice.as_deref(), process)
                    .await
                    .map_err(HandlerError::new)
            })
        }),
    }
}

/// Captures errors raised by extension-provided session callbacks so the
/// command result can reject after the runtime transition completes.
#[derive(Clone, Default)]
struct CallbackErrors(Arc<Mutex<Option<HandlerError>>>);

impl CallbackErrors {
    fn record(&self, error: HandlerError) {
        let mut slot = self.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if slot.is_none() {
            *slot = Some(error);
        }
    }

    fn take(&self) -> Result<(), HandlerError> {
        match self.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner).take() {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }

    fn adapt_setup(&self, setup: crate::core::extensions::types::SessionSetupCallback) -> RuntimeSessionSetupCallback {
        let errors = self.clone();
        Arc::new(move |manager: &mut SessionManager| {
            let future = setup(manager);
            let errors = errors.clone();
            Box::pin(async move {
                if let Err(error) = future.await {
                    errors.record(error);
                }
            })
        })
    }

    fn adapt_with_session(&self, with_session: WithSessionCallback) -> ReplacedSessionCallback {
        let errors = self.clone();
        Arc::new(move |context| {
            let future = with_session(context);
            let errors = errors.clone();
            Box::pin(async move {
                if let Err(error) = future.await {
                    errors.record(error);
                }
            })
        })
    }
}

fn lock(session: &SharedSession) -> std::sync::MutexGuard<'_, AgentSession> {
    session.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}
