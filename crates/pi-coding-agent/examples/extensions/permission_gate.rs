//! Opt-in native port of `permission-gate.ts`; register its factory explicitly.
use pi_coding_agent::core::extensions::*;
use pi_js_compat::js_format;
use pi_js_compat::js_json::JsString;
use pi_js_compat::js_json::JsValue;
use pi_js_compat::js_regex::JsRegex as Regex;
use std::sync::Arc;
pub fn permission_gate_extension(pi: &ExtensionApi) -> Result<(), HandlerError> {
    // Match non-Unicode JS /i and word boundaries, the exact JS whitespace
    // class, and JS dot's four excluded line terminators (not just Rust's LF).
    let patterns = [
        r"(?i-u:\brm)[\t\n\x0b\x0c\r \u{a0}\u{1680}\u{2000}-\u{200a}\u{2028}\u{2029}\u{202f}\u{205f}\u{3000}\u{feff}]+(?i-u:-rf?|--recursive)",
        r"(?i-u:\bsudo\b)",
        r"(?i-u:\b(chmod|chown)\b)[^\n\r\u{2028}\u{2029}]*777",
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
                    .and_then(JsValue::as_string)
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
                            &js_format!("⚠️ Dangerous command:\n\n  {}\n\nAllow?", command),
                            &["Yes".into(), "No".into()],
                            None,
                        )
                        .await?;
                    if choice.as_ref() == Some(&JsString::from("Yes")) {
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
