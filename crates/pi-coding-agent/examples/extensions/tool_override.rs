//! Opt-in audited read override. Logging failures intentionally match the TS example.
use pi_agent_core::AgentToolResult;
use pi_ai::types::{TextContent, TextOrImageContent};
use pi_coding_agent::{
    config::get_agent_dir,
    core::{extensions::*, tools::file_mutation_queue::with_file_mutation_queue},
    utils::paths::node_resolve,
};
use pi_js_compat::js_format;
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::JsString;
use pi_js_compat::js_json::JsValue as Value;
use pi_js_compat::js_json::encode_node_utf8;
use pi_js_compat::{date, node_fs::node_fs_error_record, string::js_trim};
use std::{io::Write, sync::Arc};

pub fn tool_override_extension(pi: &ExtensionApi) -> Result<(), HandlerError> {
    register_with_clock(pi, Arc::new(|| date::js_now_millis() as i64))
}

pub(crate) fn register_with_clock(
    pi: &ExtensionApi,
    clock: Arc<dyn Fn() -> i64 + Send + Sync>,
) -> Result<(), HandlerError> {
    let log = pi_coding_agent::utils::paths::node_join(get_agent_dir()?, "read-access.log");
    let log_execute = log.clone();
    let blocked = [
        (r"\.env$|\.env\..+$", ""),
        (
            r"secrets?\.(json|yaml|yml|toml)$|credentials?\.(json|yaml|yml|toml)$",
            "i",
        ),
        (r"/\.ssh/|/\.aws/|/\.gnupg/", ""),
    ]
    .into_iter()
    .map(|(pattern, flags)| {
        pi_js_compat::js_regex::JsRegex::with_flags(pattern, flags).expect("constant blocked-path pattern")
    })
    .collect::<Vec<_>>();

    pi.register_tool(ToolDefinition {
        name:"read".into(),label:"read (audited)".into(),
        description:"Read the contents of a file with access logging. Some sensitive paths (.env, secrets, credentials) are blocked.".into(),
        parameters:serde_json::json!({"type":"object","properties":{"path":{"type":"string","description":"Path to the file to read (relative or absolute)"},"offset":{"type":"number","description":"Line number to start reading from (1-indexed)"},"limit":{"type":"number","description":"Maximum number of lines to read"}},"required":["path"]}),
        prompt_snippet:None,prompt_guidelines:None,approval_required:None,approval_kind:None,render_shell:None,prepare_arguments:None,prepare_execution:None,execution_mode:None,render_call:None,render_result:None,
        execute:Arc::new(move|_,params,_,_,ctx|{
            let log=log_execute.clone();let clock=clock.clone();let blocked=blocked.clone();
            Box::pin(async move{
let ctx = pi_coding_agent::core::extensions::types::require_extension_context(ctx)?;
                let path=params["path"].as_string().ok_or_else(||HandlerError::new("path must be a string"))?;
                let absolute=node_resolve(&ctx.cwd()?,path).map_err(|error| HandlerError::new(error.message()))?;
                let denied=blocked.iter().any(|pattern|pattern.is_match(&absolute));
                log_access(&log,&absolute,denied,clock()).await;
                if denied {
                    return Ok(result(js_format!("Access denied: \"{}\" matches a blocked pattern (sensitive file). This tool blocks access to .env files, secrets, credentials, and SSH/AWS/GPG directories.", path),json!({"blocked":true})));
                }
                Ok(match read_text(&absolute) {
                    Ok(content)=>{
                        let lines:Vec<_>=content.split('\n').collect();
                        let offset=params["offset"].as_f64().unwrap_or(0.0);
                        let start=if offset!=0.0 {(offset-1.0).max(0.0)}else{0.0};
                        let limit=params["limit"].as_f64().unwrap_or(0.0);
                        let end=if limit!=0.0 {start+limit}else{lines.len() as f64};
                        let a=slice_index(start,lines.len());let b=slice_index(end,lines.len());
                        let mut text=if b>a {JsString::join(&lines[a..b], "\n")}else{JsString::default()};
                        if text.len()>10*1024 {text=js_format!("{}\n\n[Output truncated at 10KB]",text.slice(0,10*1024));}
                        result(text,json!({"lines":lines.len()}))
                    }
                    Err(message)=>result(js_format!("Error reading file: {}", message),json!({"error":true})),
                })
            })
        }),
    })?;
    pi.register_command(
        "read-log",
        Some("View the file access log".into()),
        None,
        Arc::new(move |_, ctx| {
            let log = log.clone();
            Box::pin(async move {
                // Frozen tool-override.ts:135-142 catches reading AND the first notification.
                let shown = (|| -> Result<(), pi_js_compat::callback_error::CallbackError> {
                    let bytes = std::fs::read(encode_node_utf8(&log))?;
                    let content = String::from_utf8_lossy(&bytes);
                    let lines: Vec<_> = js_trim(&content).split('\n').collect();
                    let message = format!(
                        "Recent file access:\n{}",
                        lines[lines.len().saturating_sub(20)..].join("\n")
                    );
                    ctx.ui()
                        .map_err(pi_js_compat::callback_error::CallbackError::from)?
                        .notify(&JsString::from(&message), Some(NotifyType::Info))
                })();
                if shown.is_err() {
                    ctx.ui()?
                        .notify(&JsString::from("No access log found"), Some(NotifyType::Info))?;
                }
                Ok(())
            })
        }),
    )
    .map_err(HandlerError::new)
}

fn result(text: impl Into<JsString>, details: Value) -> AgentToolResult {
    AgentToolResult {
        content: vec![TextOrImageContent::Text(TextContent::new(text))],
        details: Some(details),
        is_error: None,
        terminate: None,
    }
}
fn slice_index(value: f64, len: usize) -> usize {
    let index = value.trunc();
    if index < 0.0 {
        (len as f64 + index).max(0.0) as usize
    } else {
        (index as usize).min(len)
    }
}
fn read_text(path: &JsString) -> Result<JsString, JsString> {
    rustix::fs::access(encode_node_utf8(path), rustix::fs::Access::READ_OK)
        .map_err(|e| node_fs_error_record(&e.into(), "access", &JsString::from(path)).message)?;
    std::fs::read(encode_node_utf8(path))
        .map(|bytes| JsString::from(String::from_utf8_lossy(&bytes).as_ref()))
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::IsADirectory {
                js_format!("EISDIR: illegal operation on a directory, read '{}'", path)
            } else {
                node_fs_error_record(&e, "open", &JsString::from(path)).message
            }
        })
}
async fn log_access(path: &JsString, absolute: &JsString, denied: bool, millis: i64) {
    let status = if denied { "BLOCKED" } else { "ALLOWED" };
    let reason = if denied { " (matches blocked pattern)" } else { "" };
    let line = js_format!("[{}] {}: {}{}\n", date::to_iso_string(millis), status, absolute, reason);
    // Source explicitly ignores both queue and append failures; read must continue.
    if let Ok(Ok(())) = with_file_mutation_queue(path, || async {
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(encode_node_utf8(path))?;
        file.write_all(encode_node_utf8(&line).as_bytes())
    })
    .await
    {}
}
