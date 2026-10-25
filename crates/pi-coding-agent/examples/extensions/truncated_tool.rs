//! Opt-in ripgrep example. Like pinned execSync, its command is shell-joined.
use pi_agent_core::AgentToolResult;
use pi_ai::types::{TextContent, TextOrImageContent};
use pi_coding_agent::core::{
    extensions::*,
    tools::{file_mutation_queue::with_file_mutation_queue, truncate::*},
};
use pi_coding_agent::modes::interactive::theme::{Theme, ThemeColor as C};
use pi_js_compat::js_format;
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::JsString;
use pi_js_compat::js_json::JsValue;
use pi_js_compat::js_json::JsValue as Value;
use pi_tui::{
    Text,
    tui::{ComponentRef, component},
};
use std::sync::Arc;

pub fn truncated_tool_extension(pi: &ExtensionApi) -> Result<(), HandlerError> {
    pi.register_tool(ToolDefinition {
        name:"rg".into(),label:"ripgrep".into(),
        description:js_format!("Search file contents using ripgrep. Output is truncated to {} lines or {} (whichever is hit first). If truncated, full output is saved to a temp file.", DEFAULT_MAX_LINES, format_size(DEFAULT_MAX_BYTES as f64)),
        parameters:serde_json::json!({"type":"object","properties":{"pattern":{"type":"string","description":"Search pattern (regex)"},"path":{"type":"string","description":"Directory to search (default: current directory)"},"glob":{"type":"string","description":"File glob pattern, e.g. '*.ts'"}},"required":["pattern"]}),
        prompt_snippet:None,prompt_guidelines:None,approval_required:None,approval_kind:None,render_shell:None,prepare_arguments:None,prepare_execution:None,execution_mode:None,
        execute:Arc::new(|_,params,_,_,ctx|Box::pin(async move {
let ctx = pi_coding_agent::core::extensions::types::require_extension_context(ctx)?;execute(&params,&ctx.cwd()?).await})),
        render_call:Some(infallible_render_call(|args,theme,_|{
            let mut text=theme.fg(C::ToolTitle,&theme.bold("rg "))+&theme.fg(C::Accent,&js_format!("\"{}\"", args["pattern"].as_string().unwrap_or(&JsString::from("undefined"))));
            if let Some(path)=nonempty(args,"path"){text.push_str(&theme.fg(C::Muted,&js_format!(" in {}", path)));}
            if let Some(glob)=nonempty(args,"glob"){text.push_str(&theme.fg(C::Dim,&js_format!(" --glob {}", glob)));}
            text_component(text)
        })),
        render_result:Some(infallible_render_result(|result,options,theme,_|text_component(render_result(result,options,theme)))),
    })
}
fn nonempty<'a>(args: &'a Value, key: &str) -> Option<&'a JsString> {
    args[key].as_string().filter(|s| !s.is_empty())
}
async fn execute(params: &Value, cwd: &JsString) -> Result<AgentToolResult, HandlerError> {
    let pattern = params["pattern"]
        .as_string()
        .ok_or_else(|| HandlerError::new("pattern must be a string"))?;
    let mut args = vec![
        JsString::from("rg"),
        JsString::from("--line-number"),
        JsString::from("--color=never"),
    ];
    if let Some(glob) = nonempty(params, "glob") {
        args.extend([JsString::from("--glob"), glob.clone()]);
    }
    args.extend([
        pattern.clone(),
        nonempty(params, "path").cloned().unwrap_or_else(|| JsString::from(".")),
    ]);
    let command = JsString::join(args, " ");
    let output = run_search(&command, cwd).await?;
    let mut details = json!({"pattern":pattern});
    for key in ["path", "glob"] {
        if let Some(value) = params.get(key) {
            details[key] = value.clone();
        }
    }
    if output.status.code() == Some(1) {
        details["matchCount"] = json!(0);
        return Ok(result("No matches found".into(), details));
    }
    if !output.status.success() {
        return Err(HandlerError::new(js_format!(
            "ripgrep failed: Command failed: {}\n{}",
            command,
            String::from_utf8_lossy(&output.stderr).as_ref()
        )));
    }
    let output = String::from_utf8_lossy(&output.stdout);
    if pi_js_compat::string::js_trim(&output).is_empty() {
        details["matchCount"] = json!(0);
        return Ok(result("No matches found".into(), details));
    }
    let truncation = truncate_head(
        &JsString::from(output.as_ref()),
        TruncationOptions {
            max_lines: DEFAULT_MAX_LINES as f64,
            max_bytes: DEFAULT_MAX_BYTES as f64,
        },
    );
    details["matchCount"] = json!(
        output
            .split('\n')
            .filter(|line| !pi_js_compat::string::js_trim(line).is_empty())
            .count()
    );
    let mut text = truncation.content.clone();
    if truncation.truncated {
        let directory = tempfile::Builder::new()
            .prefix("pi-rg-")
            .tempdir()
            .map_err(|e| HandlerError::new(e.to_string()))?
            .keep();
        let path = directory.join("output.txt").display().to_string();
        with_file_mutation_queue(&JsString::from(&path), || async {
            std::fs::write(&path, output.as_bytes())
        })
        .await
        .map_err(|e| HandlerError::new(e.to_string()))?
        .map_err(|e| HandlerError::new(e.to_string()))?;
        details["truncation"] =
            JsValue::from(serde_json::to_value(&truncation).map_err(|e| HandlerError::new(e.to_string()))?);
        details["fullOutputPath"] = json!(path);
        text.push_str(&js_format!("\n\n[Output truncated: showing {} of {} lines ({} of {}). {} lines ({}) omitted. Full output saved to: {}]", truncation.output_lines, truncation.total_lines, format_size(truncation.output_bytes as f64), format_size(truncation.total_bytes as f64), truncation.total_lines - truncation.output_lines, format_size((truncation.total_bytes - truncation.output_bytes) as f64), path));
    }
    Ok(result(text, details))
}
fn result(text: JsString, details: Value) -> AgentToolResult {
    AgentToolResult {
        content: vec![TextOrImageContent::Text(TextContent::new(text))],
        details: Some(JsValue::from(details)),
        is_error: None,
        terminate: None,
    }
}
fn text_component(text: JsString) -> ComponentRef {
    component(Text::new(text, 0, 0))
}
fn render_result(result: &AgentToolResult, options: &ToolRenderResultOptions, theme: &Theme) -> JsString {
    if options.is_partial {
        return theme.fg(C::Warning, "Searching...");
    }
    let Some(details) = result
        .details
        .as_ref()
        .filter(|d| !d.is_null() && d["matchCount"].as_u64() != Some(0))
    else {
        return theme.fg(C::Dim, "No matches found");
    };
    let mut text = theme.fg(
        C::Success,
        &js_format!(
            "{} matches",
            pi_js_compat::number::js_number_to_string(details["matchCount"].as_f64().expect("numeric match count"))
        ),
    );
    if details["truncation"]["truncated"] == pi_js_compat::js_json!(true) {
        text.push_str(&theme.fg(C::Warning, " (truncated)"));
    }
    if !options.expanded {
        return text;
    }
    if let Some(TextOrImageContent::Text(content)) = result.content.first() {
        let lines: Vec<_> = content.text.split('\n').collect();
        for line in lines.iter().take(20) {
            text.push_str(&js_format!("\n{}", theme.fg(C::Dim, line)));
        }
        if lines.len() > 20 {
            text.push_str(&js_format!(
                "\n{}",
                theme.fg(C::Muted, "... (use read tool to see full output)")
            ));
        }
    }
    if let Some(path) = details["fullOutputPath"].as_string().filter(|p| !p.is_empty()) {
        text.push_str(&js_format!(
            "\n{}",
            theme.fg(C::Dim, &js_format!("Full output: {}", path))
        ));
    }
    text
}

async fn run_search(command: &JsString, cwd: &JsString) -> Result<std::process::Output, HandlerError> {
    use std::{os::unix::process::CommandExt, process::Stdio};
    use tokio::io::AsyncReadExt;
    let mut process = tokio::process::Command::new("/bin/sh");
    process
        .args(["-c", &pi_js_compat::js_json::encode_node_utf8(command)])
        .current_dir(pi_js_compat::js_json::encode_node_utf8(cwd))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    process.as_std_mut().process_group(0);
    let mut child = process
        .spawn()
        .map_err(|e| HandlerError::new(js_format!("ripgrep failed: {}", e.to_string())))?;
    let mut out = child.stdout.take().expect("piped stdout");
    let mut err = child.stderr.take().expect("piped stderr");
    let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
    let (mut out_done, mut err_done) = (false, false);
    let (mut out_buf, mut err_buf) = ([0u8; 65536], [0u8; 65536]);
    while !out_done || !err_done {
        tokio::select! {
            read=out.read(&mut out_buf),if !out_done=>{let n=read.map_err(|e|HandlerError::new(e.to_string()))?;out_done=n==0;stdout.extend_from_slice(&out_buf[..n]);}
            read=err.read(&mut err_buf),if !err_done=>{let n=read.map_err(|e|HandlerError::new(e.to_string()))?;err_done=n==0;stderr.extend_from_slice(&err_buf[..n]);}
        }
        if stdout.len() + stderr.len() > 100 * 1024 * 1024 {
            if let Some(pid) = child.id().and_then(|id| rustix::process::Pid::from_raw(id as i32)) {
                match rustix::process::kill_process_group(pid, rustix::process::Signal::KILL) {
                    Ok(()) | Err(rustix::io::Errno::SRCH) => {}
                    Err(e) => {
                        return Err(HandlerError::new(js_format!(
                            "ripgrep buffer cleanup failed: {}",
                            e.to_string()
                        )));
                    }
                }
            }
            child.wait().await.map_err(|e| HandlerError::new(e.to_string()))?;
            return Err(HandlerError::new("ripgrep failed: spawnSync /bin/sh ENOBUFS"));
        }
    }
    let status = child.wait().await.map_err(|e| HandlerError::new(e.to_string()))?;
    Ok(std::process::Output { status, stdout, stderr })
}
