//! Opt-in native port of `permission-gate.ts`; register its factory explicitly.
use pi_coding_agent::core::extensions::*;
use regex::Regex;
use std::sync::Arc;
pub fn permission_gate_extension(pi: &ExtensionApi) -> Result<(), HandlerError> {
    // JavaScript's word boundaries are ASCII; its whitespace also includes BOM.
    let patterns = [
        r"(?i)(?-u:\b)rm[\s\u{feff}]+(-rf?|--recursive)",
        r"(?i)(?-u:\b)sudo(?-u:\b)",
        r"(?i)(?-u:\b)(chmod|chown)(?-u:\b).*777",
    ];
    let patterns = Arc::new(patterns.map(|pattern| Regex::new(pattern).expect("constant JS-compatible pattern")));
    pi.on_tool_call(
        move |event: ToolCallEvent,
              ctx: ExtensionContext|
              -> BoxFuture<'static, Result<Option<ToolCallEventResult>, HandlerError>> {
            let patterns = patterns.clone();
            Box::pin(async move {
                if event.tool_name != ToolName::Bash {
                    return Ok(None);
                }
                let command = event
                    .input
                    .get("command")
                    .and_then(serde_json::Value::as_str)
                    .ok_or_else(|| HandlerError::new("command must be a string"))?;
                if !patterns.iter().any(|pattern| pattern.is_match(command)) {
                    return Ok(None);
                }
                let reason = if !ctx.has_ui()? {
                    "Dangerous command blocked (no UI for confirmation)"
                } else {
                    let choice = ctx
                        .ui()?
                        .select(
                            &format!("⚠️ Dangerous command:\n\n  {command}\n\nAllow?"),
                            &["Yes".into(), "No".into()],
                            None,
                        )
                        .await?;
                    if choice.as_deref() == Some("Yes") {
                        return Ok(None);
                    }
                    "Blocked by user"
                };
                Ok(Some(ToolCallEventResult {
                    block: Some(true),
                    reason: Some(reason.into()),
                    ..Default::default()
                }))
            })
        },
    )
    .map_err(HandlerError::new)
}
