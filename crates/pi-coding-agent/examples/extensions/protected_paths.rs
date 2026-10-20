//! Opt-in native port of `protected-paths.ts`; register its factory explicitly.
use pi_coding_agent::core::extensions::*;
pub fn protected_paths_extension(pi: &ExtensionApi) -> Result<(), HandlerError> {
    pi.on_tool_call(
        |event: ToolCallEvent,
         ctx: ExtensionContext|
         -> BoxFuture<'static, Result<Option<ToolCallEventResult>, HandlerError>> {
            Box::pin(async move {
                if !matches!(event.tool_name, ToolName::Write | ToolName::Edit) {
                    return Ok(None);
                }
                let path = event
                    .input
                    .get("path")
                    .and_then(serde_json::Value::as_str)
                    .ok_or_else(|| HandlerError::new("path must be a string"))?;
                if ![".env", ".git/", "node_modules/"]
                    .iter()
                    .any(|part| path.contains(part))
                {
                    return Ok(None);
                }
                if ctx.has_ui()? {
                    ctx.ui()?.notify(
                        &format!("Blocked write to protected path: {path}"),
                        Some(NotifyType::Warning),
                    );
                }
                Ok(Some(ToolCallEventResult {
                    block: Some(true),
                    reason: Some(format!("Path \"{path}\" is protected")),
                    ..Default::default()
                }))
            })
        },
    )
    .map_err(HandlerError::new)
}
