//! Native session transitions and isolated summary takeovers.
use super::provider::BridgeProvider;
use crate::core::{
    compaction::branch_summarization::{GenerateBranchSummaryOptions, generate_branch_summary},
    extensions::types::*,
};
use pi_agent_core::agent::StreamFn;
use pi_js_compat::js_json::{JsString, JsValue};
use std::sync::Arc;

fn isolated_stream(bridge: BridgeProvider, cwd: JsString) -> StreamFn {
    Arc::new(move |model, context, mut options| {
        options.stream.cwd = Some(cwd.clone());
        let stream = super::summaries::stream(bridge.clone(), model, context, options);
        Box::pin(async move { Ok(stream) })
    })
}
pub(super) fn register(pi: &ExtensionApi, bridge: BridgeProvider) -> Result<(), HandlerError> {
    let start_bridge = bridge.clone();
    pi.on_session_start(
        move |event: SessionStartEvent,
              ctx: ExtensionContext|
              -> BoxFuture<'static, Result<Option<()>, HandlerError>> {
            let ui = ctx.ui();
            if let Ok(ui) = &ui {
                *start_bridge
                    .ui
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(ui.clone());
            }
            let result = ui.and_then(|_| {
                if matches!(
                    event.reason,
                    SessionStartReason::New | SessionStartReason::Resume | SessionStartReason::Fork
                ) {
                    start_bridge.clear_sessions().map_err(HandlerError::from)
                } else {
                    Ok(())
                }
            });
            Box::pin(async { result.map(|()| None) })
        },
    )
    .map_err(HandlerError::new)?;
    let compact_bridge = bridge.clone();
    pi.on_session_compact(
        move |_, ctx: ExtensionContext| -> BoxFuture<'static, Result<Option<()>, HandlerError>> {
            let bridge = compact_bridge.clone();
            Box::pin(async move {
                bridge.mark_rebuild(&ctx.session_manager()?.get_session_id(), false)?;
                Ok(None)
            })
        },
    )
    .map_err(HandlerError::new)?;
    let shutdown_bridge = bridge.clone();
    pi.on_session_shutdown(
        move |_, ctx: ExtensionContext| -> BoxFuture<'static, Result<Option<()>, HandlerError>> {
            let bridge = shutdown_bridge.clone();
            Box::pin(async move {
                bridge
                    .shutdown(&ctx.session_manager()?.get_session_id(), &"pi session shut down".into())
                    .await?;
                bridge.clear_sessions()?;
                Ok(None)
            })
        },
    )
    .map_err(HandlerError::new)?;
    let tree_bridge = bridge.clone();
    pi.on_session_tree(
        move |_, ctx: ExtensionContext| -> BoxFuture<'static, Result<Option<()>, HandlerError>> {
            let bridge = tree_bridge.clone();
            Box::pin(async move {
                bridge.mark_rebuild(&ctx.session_manager()?.get_session_id(), false)?;
                Ok(None)
            })
        },
    )
    .map_err(HandlerError::new)?;
    pi.on_session_before_tree(
        move |event: SessionBeforeTreeEvent,
              ctx: ExtensionContext|
              -> BoxFuture<'static, Result<Option<SessionBeforeTreeResult>, HandlerError>> {
            let bridge = bridge.clone();
            Box::pin(async move { take_over_branch_summary(bridge, event, ctx).await })
        },
    )
    .map_err(HandlerError::new)?;
    Ok(())
}
async fn take_over_branch_summary(
    bridge: BridgeProvider,
    event: SessionBeforeTreeEvent,
    ctx: ExtensionContext,
) -> Result<Option<SessionBeforeTreeResult>, HandlerError> {
    let Some(model) = ctx.model()?.filter(|m| m.base_url == "claude-bridge") else {
        return Ok(None);
    };
    let prep = event.preparation;
    if !prep.user_wants_summary || prep.entries_to_summarize.is_empty() {
        return Ok(None);
    }
    let stream = isolated_stream(bridge, ctx.cwd()?);
    let result = generate_branch_summary(
        &prep.entries_to_summarize,
        &GenerateBranchSummaryOptions {
            model: &model,
            api_key: None,
            headers: None,
            env: None,
            signal: event.signal,
            custom_instructions: prep.custom_instructions.as_ref(),
            replace_instructions: prep.replace_instructions.unwrap_or(false),
            reserve_tokens: None,
            stream_fn: Some(&stream),
        },
    )
    .await;
    match result {
        Ok(result) if result.aborted == Some(true) => Ok(Some(SessionBeforeTreeResult {
            cancel: Some(true),
            ..Default::default()
        })),
        Ok(result) if result.error.is_none() => {
            let details = JsValue::object([
                (
                    "readFiles",
                    JsValue::Array(
                        result
                            .read_files
                            .unwrap_or_default()
                            .into_iter()
                            .map(JsValue::String)
                            .collect(),
                    ),
                ),
                (
                    "modifiedFiles",
                    JsValue::Array(
                        result
                            .modified_files
                            .unwrap_or_default()
                            .into_iter()
                            .map(JsValue::String)
                            .collect(),
                    ),
                ),
            ]);
            Ok(Some(SessionBeforeTreeResult {
                summary: Some(SessionTreeSummary {
                    summary: result.summary.unwrap_or_default(),
                    details: Some(details),
                    usage: None,
                }),
                ..Default::default()
            }))
        }
        result => {
            let error = match result {
                Ok(result) => result.error.unwrap_or_default(),
                Err(error) => error.message().clone(),
            };
            ctx.ui()?.notify(
                &pi_js_compat::js_format!("Claude bridge branch summary failed ({}); navigation cancelled.", error),
                Some(NotifyType::Error),
            )?;
            Ok(Some(SessionBeforeTreeResult {
                cancel: Some(true),
                ..Default::default()
            }))
        }
    }
}
