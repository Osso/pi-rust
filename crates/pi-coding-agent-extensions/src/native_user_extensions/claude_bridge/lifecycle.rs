//! Native session transitions and isolated summary takeovers.
use super::provider::BridgeProvider;
use crate::core::{
    compaction::{
        CompactionSummaryOptions,
        branch_summarization::{GenerateBranchSummaryOptions, generate_branch_summary},
        compact,
    },
    extensions::types::*,
};
use pi_agent_core::agent::StreamFn;
use pi_js_compat::callback_error::CallbackError;
use pi_js_compat::js_json::{self, JsString, JsValue};
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
            let result = ui.and_then(|_| ctx.mode()).and_then(|mode| {
                *start_bridge
                    .mode
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(mode);
                match event.reason {
                    SessionStartReason::New => start_bridge.clear_sessions("session_start:new"),
                    SessionStartReason::Resume => start_bridge.clear_sessions("session_start:resume"),
                    SessionStartReason::Fork => start_bridge.clear_sessions("session_start:fork"),
                    _ => Ok(()),
                }
                .map_err(HandlerError::from)
            });
            let result = result.and_then(|()| {
                if start_bridge.settings.get("shapeTrace").and_then(JsValue::as_bool) == Some(true) {
                    let session = ctx.session_manager()?.get_session_id();
                    let mut trace = super::shape_trace::ShapeTrace::new(
                        super::logging::agent_dir()?.join("claude-bridge-shapes.jsonl"),
                        4 * 1024 * 1024,
                        Some(&session),
                    )?;
                    trace.record_pi("session_start", &JsValue::Array(Vec::new()))?;
                }
                Ok(())
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
                bridge.mark_rebuild(&ctx.session_manager()?.get_session_id(), false, "session_compact")?;
                Ok(None)
            })
        },
    )
    .map_err(HandlerError::new)?;
    // Source registers this on `session_before_compact` (index.ts:3011), whose result
    // the pinned host reduces to `cancel`; the `compaction` event honors the takeover
    // (user decision 2026-10-26).
    let takeover_bridge = bridge.clone();
    pi.on_compaction(
        move |event: CompactionEvent,
              ctx: ExtensionContext|
              -> BoxFuture<'static, Result<Option<CompactionEventResult>, HandlerError>> {
            let bridge = takeover_bridge.clone();
            Box::pin(async move { take_over_compaction(bridge, event, ctx).await })
        },
    )
    .map_err(HandlerError::new)?;
    let shutdown_bridge = bridge.clone();
    pi.on_session_shutdown(
        move |_, ctx: ExtensionContext| -> BoxFuture<'static, Result<Option<()>, HandlerError>> {
            let bridge = shutdown_bridge.clone();
            Box::pin(async move {
                bridge.report_leaks("session_shutdown")?;
                bridge
                    .shutdown(&ctx.session_manager()?.get_session_id(), &"pi session shut down".into())
                    .await?;
                bridge.clear_sessions("session_shutdown")?;
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
                bridge.mark_rebuild(&ctx.session_manager()?.get_session_id(), false, "session_tree")?;
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
fn reason_name(reason: CompactionReason) -> &'static str {
    match reason {
        CompactionReason::Idle => "idle",
        CompactionReason::Manual => "manual",
        CompactionReason::Threshold => "threshold",
        CompactionReason::Overflow => "overflow",
    }
}

/// `reinjectPriorCompactionFileOps` (index.ts:1014-1023).
fn reinject_prior_compaction_file_ops(
    logs: &super::logging::Logs,
    branch_entries: &[crate::core::session_manager::SessionEntry],
    preparation: &mut CompactionPreparation,
) -> Result<(), HandlerError> {
    let prior = branch_entries
        .iter()
        .rev()
        .find(|entry| entry.entry_type() == "compaction");
    let details = prior
        .and_then(|entry| entry.as_compaction())
        .and_then(|entry| entry.details.clone());
    let (Some(JsValue::Array(read)), Some(JsValue::Array(modified))) = (
        details.as_ref().and_then(|d| d.get("readFiles")),
        details.as_ref().and_then(|d| d.get("modifiedFiles")),
    ) else {
        return Ok(());
    };
    for file in read {
        preparation
            .file_ops
            .read
            .insert(js_json::to_js_string(file).map_err(HandlerError::new)?);
    }
    for file in modified {
        preparation
            .file_ops
            .edited
            .insert(js_json::to_js_string(file).map_err(HandlerError::new)?);
    }
    logs.debug(format!(
        "compact takeover: re-injected prior file ops read={} modified={}",
        read.len(),
        modified.len()
    ))?;
    Ok(())
}

async fn take_over_compaction(
    bridge: BridgeProvider,
    mut event: CompactionEvent,
    ctx: ExtensionContext,
) -> Result<Option<CompactionEventResult>, HandlerError> {
    let Some(model) = ctx.model()?.filter(|m| m.base_url == "claude-bridge") else {
        return Ok(None);
    };
    let logs = bridge
        .logs
        .as_ref()
        .map_err(|e| HandlerError::new(e.message.clone()))?
        .clone();
    logs.debug(format!(
        "session_before_compact: takeover reason={} willRetry={} isSplitTurn={} messages={} turnPrefix={}",
        reason_name(event.reason),
        event.will_retry,
        event.preparation.is_split_turn,
        event.preparation.messages_to_summarize.len(),
        event.preparation.turn_prefix_messages.len()
    ))?;
    let result = match reinject_prior_compaction_file_ops(&logs, &event.branch_entries, &mut event.preparation) {
        Ok(()) => {
            let stream = isolated_stream(bridge, ctx.cwd()?);
            compact(
                &event.preparation,
                &model,
                &CompactionSummaryOptions {
                    model: &model,
                    api_key: None,
                    headers: None,
                    custom_instructions: event.custom_instructions.as_ref(),
                    signal: event.signal.clone(),
                    thinking_level: None,
                    stream_fn: Some(&stream),
                    env: None,
                },
            )
            .await
            .map_err(|error| CallbackError::new(error.message().clone()))
        }
        Err(error) => Err(CallbackError::new(error.message)),
    };
    match result {
        Ok(compaction) => {
            logs.debug(format!(
                "session_before_compact: takeover complete summaryLen={}",
                compaction.summary.len()
            ))?;
            Ok(Some(CompactionEventResult {
                compaction: Some(compaction),
                ..Default::default()
            }))
        }
        Err(error) => {
            logs.debug_error(
                "session_before_compact: takeover failed; cancelling to avoid native compact fallback",
                &error,
            )?;
            ctx.ui()?.notify(
                &pi_js_compat::js_format!(
                    "Claude bridge compact failed ({}); cancelled to avoid known hang. Retry, switch model, or reduce context.",
                    error.message
                ),
                Some(NotifyType::Error),
            )?;
            Ok(Some(CompactionEventResult {
                cancel: Some(true),
                ..Default::default()
            }))
        }
    }
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
    let logs = bridge
        .logs
        .as_ref()
        .map_err(|e| HandlerError::new(e.message.clone()))?
        .clone();
    logs.debug(pi_js_compat::js_format!(
        "session_before_tree: takeover entries={} target={}",
        prep.entries_to_summarize.len(),
        prep.target_id.slice(0, 8)
    ))?;
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
            logs.debug(format!(
                "session_before_tree: takeover complete summaryLen={}",
                result.summary.as_ref().map_or(0, JsString::len)
            ))?;
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
            logs.debug_error(
                "session_before_tree: takeover failed; cancelling navigation",
                &pi_js_compat::callback_error::CallbackError::new(error.clone()),
            )?;
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
