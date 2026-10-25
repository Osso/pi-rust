//! Opt-in native `git-checkpoint.ts`; keeps stash refs only for the current run.
use pi_coding_agent::core::extensions::*;
use pi_js_compat::js_json::JsString;
use pi_js_compat::string::js_trim;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct Checkpoints {
    refs: HashMap<JsString, String>,
    current_entry_id: Option<JsString>,
}

pub fn git_checkpoint_extension(pi: &ExtensionApi) -> Result<(), HandlerError> {
    let state = Arc::new(Mutex::new(Checkpoints::default()));
    let tool_state = state.clone();
    pi.on_tool_result(
        move |_: ToolResultEvent,
              ctx: ExtensionContext|
              -> BoxFuture<'static, Result<Option<ToolResultEventResult>, HandlerError>> {
            let state = tool_state.clone();
            Box::pin(async move {
                let manager = ctx.session_manager()?;
                if let Some(leaf) = manager.get_leaf_entry() {
                    state
                        .lock()
                        .map_err(|e| HandlerError::new(e.to_string()))?
                        .current_entry_id = leaf.id().cloned();
                }
                Ok(None)
            })
        },
    )?;
    let turn_state = state.clone();
    let api = pi.clone();
    pi.on_turn_start(
        move |_: TurnStartEvent, _: ExtensionContext| -> BoxFuture<'static, Result<Option<()>, HandlerError>> {
            let state = turn_state.clone();
            let api = api.clone();
            Box::pin(async move {
                let result = api
                    .exec("git".into(), vec!["stash".into(), "create".into()], None)
                    .await?;
                let reference = js_trim(&result.stdout);
                let mut state = state.lock().map_err(|e| HandlerError::new(e.to_string()))?;
                if let Some(id) = state.current_entry_id.clone().filter(|_| !reference.is_empty()) {
                    state.refs.insert(id, reference.into());
                }
                Ok(None)
            })
        },
    )?;
    let fork_state = state.clone();
    let api = pi.clone();
    pi.on_session_before_fork(
        move |event: SessionBeforeForkEvent,
              ctx: ExtensionContext|
              -> BoxFuture<'static, Result<Option<SessionBeforeForkResult>, HandlerError>> {
            let state = fork_state.clone();
            let api = api.clone();
            Box::pin(async move {
                let reference = state
                    .lock()
                    .map_err(|e| HandlerError::new(e.to_string()))?
                    .refs
                    .get(&event.entry_id)
                    .cloned();
                let Some(reference) = reference else {
                    return Ok(None);
                };
                if !ctx.has_ui()? {
                    return Ok(None);
                }
                let choice = ctx
                    .ui()?
                    .select(
                        &JsString::from("Restore code state?"),
                        &["Yes, restore code to that point".into(), "No, keep current code".into()],
                        None,
                    )
                    .await?;
                if choice.as_ref().is_some_and(|choice| choice.starts_with("Yes")) {
                    // Source notifies even when git returns a nonzero status.
                    api.exec("git".into(), vec!["stash".into(), "apply".into(), reference], None)
                        .await?;
                    ctx.ui()?
                        .notify(&JsString::from("Code restored to checkpoint"), Some(NotifyType::Info))?;
                }
                Ok(None)
            })
        },
    )?;
    pi.on_agent_end(
        move |_: AgentEndEvent, _: ExtensionContext| -> BoxFuture<'static, Result<Option<()>, HandlerError>> {
            let state = state.clone();
            Box::pin(async move {
                state.lock().map_err(|e| HandlerError::new(e.to_string()))?.refs.clear();
                Ok(None)
            })
        },
    )
    .map_err(HandlerError::new)
}
