//! Shared extension command-context actions for the non-interactive modes.
//!
//! `modes/print-mode.ts` and `modes/rpc/rpc-mode.ts` build the same six runtime
//! callbacks; the interactive mode adds UI callbacks and is ported separately.

use std::sync::{Arc, Mutex};

use pi_ai::types::UserContent;
use tokio::sync::Mutex as AsyncMutex;

use crate::core::agent_session::{AgentSession, AgentSessionError, NavigateTreeOptions as SessionNavigateTreeOptions};
use crate::core::agent_session_runtime::{
    AgentSessionRuntime, ForkPosition, NewSessionOptions as RuntimeNewSessionOptions,
    SessionSetupCallback as RuntimeSessionSetupCallback,
};
use crate::core::extensions::types::{
    CustomMessagePayload, ExtensionCommandContextActions, ForkOptions, HandlerError, MessageDelivery,
    NavigateTreeOptions, NewSessionOptions, ReplacedSessionContext, RestartOptions, SendMessageOptions,
    SendUserMessageOptions, SessionTransitionResult, SwitchSessionOptions, UserMessageContent, UserMessageDelivery,
    WithSessionCallback,
};
use crate::core::session_manager::SessionManager;
use pi_ai::request::BoxFuture;

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
    ExtensionCommandContextActions {
        show_approval_selector: Arc::new(|| Ok(())),
        show_sandbox_selector: Arc::new(|| Ok(())),
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
                    .map_err(|error| HandlerError::new(error.message()))?;
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
                session.reload(None).await;
            })
        }),
        restart: Arc::new(move |options: Option<RestartOptions>| {
            let runtime = Arc::clone(&restart_runtime);
            Box::pin(async move {
                let (notice, process) = options.map_or((None, false), |options| {
                    (options.notice, options.process.unwrap_or(false))
                });
                let mut runtime = runtime.lock().await;
                if let Err(error) = runtime.restart(notice.as_deref(), process).await {
                    eprintln!("{error}");
                }
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

    fn adapt_with_session(&self, with_session: WithSessionCallback) -> WithSessionAdapter {
        let errors = self.clone();
        Arc::new(move |session: AgentSession| {
            let future = with_session(replaced_session_context(&session));
            let errors = errors.clone();
            Box::pin(async move {
                if let Err(error) = future.await {
                    errors.record(error);
                }
            })
        })
    }
}

type WithSessionAdapter = Arc<dyn Fn(AgentSession) -> BoxFuture<'static, ()> + Send + Sync>;

/// Fresh command-capable context bound to the replacement session.
fn replaced_session_context(session: &AgentSession) -> ReplacedSessionContext {
    let send_message_session = session.clone();
    let send_user_message_session = session.clone();
    ReplacedSessionContext {
        command: session.extension_runner().create_command_context(),
        send_message: Arc::new(
            move |message: CustomMessagePayload, options: Option<SendMessageOptions>| {
                let session = send_message_session.clone();
                Box::pin(async move {
                    let options = options.unwrap_or_default();
                    if options.deliver_as == Some(MessageDelivery::NextTurn) {
                        session.send_custom_message_next_turn(
                            &message.custom_type,
                            custom_message_content(message.content),
                            message.display,
                            message.details,
                        );
                        return Ok(());
                    }
                    session
                        .send_custom_message(
                            &message.custom_type,
                            custom_message_content(message.content),
                            message.display,
                            message.details,
                            options.trigger_turn.unwrap_or(false),
                            delivery_to_streaming_behavior(options.deliver_as),
                        )
                        .await
                        .map_err(session_error)
                })
            },
        ),
        send_user_message: Arc::new(
            move |content: UserMessageContent, options: Option<SendUserMessageOptions>| {
                let session = send_user_message_session.clone();
                Box::pin(async move {
                    let options = options.unwrap_or_default();
                    session
                        .send_user_message(
                            user_message_content(content),
                            user_delivery_to_streaming_behavior(options.deliver_as),
                            options.expand_prompt_templates.unwrap_or(true),
                        )
                        .await
                        .map_err(session_error)
                })
            },
        ),
    }
}

fn custom_message_content(
    content: crate::core::extensions::types::CustomMessageContent,
) -> pi_agent_core::CustomMessageContent {
    match content {
        crate::core::extensions::types::CustomMessageContent::Text(text) => {
            pi_agent_core::CustomMessageContent::Text(text)
        }
        crate::core::extensions::types::CustomMessageContent::Blocks(blocks) => {
            pi_agent_core::CustomMessageContent::Blocks(blocks)
        }
    }
}

fn user_message_content(content: UserMessageContent) -> UserContent {
    match content {
        UserMessageContent::Text(text) => UserContent::Text(text),
        UserMessageContent::Blocks(blocks) => UserContent::Blocks(blocks),
    }
}

fn delivery_to_streaming_behavior(
    delivery: Option<MessageDelivery>,
) -> Option<crate::core::agent_session::StreamingBehavior> {
    match delivery {
        Some(MessageDelivery::Steer) => Some(crate::core::agent_session::StreamingBehavior::Steer),
        Some(MessageDelivery::FollowUp) => Some(crate::core::agent_session::StreamingBehavior::FollowUp),
        Some(MessageDelivery::NextTurn) | None => None,
    }
}

fn user_delivery_to_streaming_behavior(
    delivery: Option<UserMessageDelivery>,
) -> Option<crate::core::agent_session::StreamingBehavior> {
    match delivery {
        Some(UserMessageDelivery::Steer) => Some(crate::core::agent_session::StreamingBehavior::Steer),
        Some(UserMessageDelivery::FollowUp) => Some(crate::core::agent_session::StreamingBehavior::FollowUp),
        None => None,
    }
}

fn session_error(error: AgentSessionError) -> HandlerError {
    HandlerError::new(error.message())
}

fn lock(session: &SharedSession) -> std::sync::MutexGuard<'_, AgentSession> {
    session.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}
