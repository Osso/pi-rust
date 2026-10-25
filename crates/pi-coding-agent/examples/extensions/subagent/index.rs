//! Opt-in native subagent tool. Host supplies its executable invocation; never auto-loaded.
pub mod agents;
mod render;
use agents::{AgentConfig, AgentDiscoveryResult, discover_agents};
use pi_agent_core::{AgentToolResult, AgentToolUpdateCallback};
use pi_ai::types::{TextContent, TextOrImageContent};
use pi_coding_agent::core::extensions::*;
use pi_js_compat::abort::AbortSignal;
use pi_js_compat::js_format;
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::JsString;
use pi_js_compat::js_json::JsValue;
use pi_js_compat::js_json::JsValue as Value;
use pi_js_compat::js_json::encode_node_utf8;
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, BufReader},
    process::Command,
};

#[derive(Clone)]
pub struct Invocation {
    pub program: PathBuf,
    pub prefix: Vec<JsString>,
}
#[derive(Clone)]
struct Dispatch {
    invocation: Invocation,
    cwd: JsString,
    agents: Vec<AgentConfig>,
}
fn text(v: &JsValue) -> JsString {
    v.as_string().cloned().unwrap_or_else(JsString::default)
}
fn empty_result(agent: &JsString, task: &JsString, code: i32) -> Value {
    json!({"agent":agent,"agentSource":"unknown","task":task,"exitCode":code,"messages":[],"stderr":"","usage":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0,"cost":0,"contextTokens":0,"turns":0}})
}
fn output(messages: &Value) -> JsString {
    for message in messages.as_array().into_iter().flatten().rev() {
        if message["role"] != pi_js_compat::js_json!("assistant") {
            continue;
        }
        for part in message["content"].as_array().into_iter().flatten() {
            if part["type"] == pi_js_compat::js_json!("text") {
                return text(&part["text"]).into();
            }
        }
    }
    JsString::default()
}
fn failed(r: &JsValue) -> bool {
    r["exitCode"] != pi_js_compat::js_json!(0)
        || r["stopReason"] == pi_js_compat::js_json!("error")
        || r["stopReason"] == pi_js_compat::js_json!("aborted")
}
fn result_output(r: &Value) -> JsString {
    if failed(r) {
        for key in ["errorMessage", "stderr"] {
            if let Some(s) = r[key].as_string().filter(|s| !s.is_empty()) {
                return s.into();
            }
        }
    }
    let s = output(&r["messages"]);
    if s.is_empty() { "(no output)".into() } else { s }
}
fn result(content: JsString, details: Value, error: bool) -> AgentToolResult {
    AgentToolResult {
        content: vec![TextOrImageContent::Text(TextContent::new(content))],
        details: Some(JsValue::from(details)),
        is_error: error.then_some(true),
        terminate: None,
    }
}
fn details(mode: &str, scope: &JsString, discovery: &AgentDiscoveryResult, results: Vec<Value>) -> Value {
    json!({"mode":mode,"agentScope":scope,"projectAgentsDir":discovery.project_agents_dir,"results":results})
}

pub fn subagent_extension(api: &ExtensionApi, agent_dir: PathBuf, invocation: Invocation) -> Result<(), HandlerError> {
    let item = serde_json::json!({"type":"object","properties":{"agent":{"type":"string","description":"Name of the agent to invoke"},"task":{"type":"string","description":"Task to delegate to the agent"},"cwd":{"type":"string","description":"Working directory for the agent process"}},"required":["agent","task"]});
    let mut chain = item.clone();
    chain["properties"]["task"]["description"] =
        serde_json::json!("Task with optional {previous} placeholder for prior output");
    api.register_tool(ToolDefinition {
 name:"subagent".into(),label:"Subagent".into(),description:js_format!("Delegate tasks to specialized subagents with isolated context. Modes: single (agent + task), parallel (tasks array), chain (sequential with {} placeholder). Default agent scope is \"user\" (from {}). To enable project-local agents in {}/agents, set agentScope: \"both\" (or \"project\").", "{previous}", pi_js_compat::node_fs::decode_node_path(agent_dir.join("agents").as_os_str()), pi_coding_agent::config::config_dir_name()),
 parameters:serde_json::json!({"type":"object","properties":{"agent":{"type":"string","description":"Name of the agent to invoke (for single mode)"},"task":{"type":"string","description":"Task to delegate (for single mode)"},"tasks":{"type":"array","items":item,"description":"Array of {agent, task} for parallel execution"},"chain":{"type":"array","items":chain,"description":"Array of {agent, task} for sequential execution"},"agentScope":{"type":"string","enum":["user","project","both"],"description":"Which agent directories to use. Default: \"user\". Use \"both\" to include project-local agents.","default":"user"},"confirmProjectAgents":{"type":"boolean","description":"Prompt before running project-local agents. Default: true.","default":true},"cwd":{"type":"string","description":"Working directory for the agent process (single mode)"}}}),
 prompt_snippet:None,prompt_guidelines:None,approval_required:None,approval_kind:None,constrained_sampling:None,render_shell:None,prepare_arguments:None,prepare_execution:None,execution_mode:None,
 execute:Arc::new(move|_,params,signal,update,ctx|{let dir=agent_dir.clone();let invocation=invocation.clone();Box::pin(async move{execute(params,signal,update,pi_coding_agent::core::extensions::types::require_extension_context(ctx)?,dir,invocation).await})}),
 render_call:Some(infallible_render_call(render::call)),render_result:Some(infallible_render_result(render::result)),
 })
}
async fn execute(
    params: Value,
    signal: Option<AbortSignal>,
    update: Option<AgentToolUpdateCallback>,
    ctx: ExtensionContext,
    dir: PathBuf,
    invocation: Invocation,
) -> Result<AgentToolResult, HandlerError> {
    let scope = params["agentScope"]
        .as_string()
        .cloned()
        .unwrap_or_else(|| JsString::from("user"));
    let cwd = ctx.cwd()?;
    let discovery = discover_agents(Path::new(&encode_node_utf8(&cwd)), &scope, &dir).map_err(HandlerError::new)?;
    let dispatch = Dispatch {
        invocation,
        cwd,
        agents: discovery.agents.clone(),
    };
    let chain = params["chain"].as_array().filter(|v| !v.is_empty());
    let tasks = params["tasks"].as_array().filter(|v| !v.is_empty());
    let single = !text(&params["agent"]).is_empty() && !text(&params["task"]).is_empty();
    if usize::from(chain.is_some()) + usize::from(tasks.is_some()) + usize::from(single) != 1 {
        let available = JsString::join(
            discovery
                .agents
                .iter()
                .map(|a| js_format!("{} ({})", a.name, a.source))
                .collect::<Vec<_>>(),
            ", ",
        );
        return Ok(result(
            js_format!(
                "Invalid parameters. Provide exactly one mode.\nAvailable agents: {}",
                if available.is_empty() {
                    JsString::from("none")
                } else {
                    available
                }
            ),
            details("single", &scope, &discovery, vec![]),
            false,
        ));
    }
    let mode = if chain.is_some() {
        "chain"
    } else if tasks.is_some() {
        "parallel"
    } else {
        "single"
    };
    if scope != "user" && params["confirmProjectAgents"].as_bool().unwrap_or(true) && ctx.has_ui()? {
        let mut names = Vec::<JsString>::new();
        for item in chain.into_iter().chain(tasks).flatten().chain(std::iter::once(&params)) {
            let name = text(&item["agent"]);
            if !name.is_empty() && !names.iter().any(|n| n == &name) {
                names.push(name.into());
            }
        }
        let names = names
            .iter()
            .filter(|n| discovery.agents.iter().any(|a| &a.name == *n && a.source == "project"))
            .cloned()
            .collect::<Vec<_>>();
        if !names.is_empty() {
            let dir = discovery
                .project_agents_dir
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "(unknown)".into());
            if !ctx.ui()?.confirm(&JsString::from("Run project-local agents?"),&JsString::from(js_format!("Agents: {}\nSource: {}\n\nProject agents are repo-controlled. Only continue for trusted repositories.", JsString::join(&names, ", "), dir)),None).await? {
    return Ok(result("Canceled: project-local agents not approved.".into(),details(mode,&scope,&discovery,vec![]),false));
   }
        }
    }
    let base = details(mode, &scope, &discovery, vec![]);
    if let Some(chain) = chain {
        return run_chain(&dispatch, chain, &signal, &update, base).await;
    }
    if let Some(tasks) = tasks {
        return run_parallel(&dispatch, tasks, &signal, &update, base).await;
    }
    let r = run_single(&dispatch, &params, None, signal, update, base.clone()).await?;
    let is_error = failed(&r);
    let content = if is_error {
        js_format!(
            "Agent {}: {}",
            r["stopReason"].as_string().unwrap_or(&JsString::from("failed")),
            result_output(&r)
        )
    } else {
        result_output(&r)
    };
    let mut d = base;
    d["results"] = json!([r]);
    Ok(result(content, d, is_error))
}
async fn run_chain(
    dispatch: &Dispatch,
    chain: &[Value],
    signal: &Option<AbortSignal>,
    update: &Option<AgentToolUpdateCallback>,
    mut base: Value,
) -> Result<AgentToolResult, HandlerError> {
    let mut results = Vec::<Value>::new();
    let mut previous = JsString::default();
    for (i, item) in chain.iter().enumerate() {
        let mut item = item.clone();
        item["task"] = json!(pi_js_compat::string::replace_global(
            text(&item["task"]),
            "{previous}",
            &previous
        ));
        let prior = results.clone();
        let d = base.clone();
        let callback = update.as_ref().map(|u| {
            let u = u.clone();
            Arc::new(move |mut partial: AgentToolResult| {
                let mut all = prior.clone();
                if let Some(r) = partial
                    .details
                    .as_ref()
                    .and_then(|d| d["results"].as_array())
                    .and_then(|r| r.first())
                {
                    all.push(r.clone());
                }
                let mut d = d.clone();
                d["results"] = json!(all);
                partial.details = Some(JsValue::from(d));
                u(partial)
            }) as AgentToolUpdateCallback
        });
        let r = run_single(dispatch, &item, Some(i + 1), signal.clone(), callback, base.clone()).await?;
        previous = output(&r["messages"]);
        results.push(r.clone());
        if failed(&r) {
            base["results"] = json!(results);
            return Ok(result(
                js_format!(
                    "Chain stopped at step {} ({}): {}",
                    i + 1,
                    text(&r["agent"]),
                    result_output(&r)
                ),
                base,
                true,
            ));
        }
    }
    base["results"] = json!(results);
    Ok(result(
        if previous.is_empty() {
            "(no output)".into()
        } else {
            previous
        },
        base,
        false,
    ))
}
async fn run_parallel(
    dispatch: &Dispatch,
    tasks: &[Value],
    signal: &Option<AbortSignal>,
    update: &Option<AgentToolUpdateCallback>,
    mut base: Value,
) -> Result<AgentToolResult, HandlerError> {
    if tasks.len() > 8 {
        return Ok(result(
            js_format!("Too many parallel tasks ({}). Max is 8.", tasks.len()),
            base,
            false,
        ));
    }
    let all = Arc::new(Mutex::new(
        tasks
            .iter()
            .map(|t| empty_result(&text(&t["agent"]), &text(&t["task"]), -1))
            .collect::<Vec<_>>(),
    ));
    let semaphore = Arc::new(tokio::sync::Semaphore::new(4));
    let mut jobs = Vec::new();
    for (index, item) in tasks.iter().cloned().enumerate() {
        let dispatch = dispatch.clone();
        let semaphore = semaphore.clone();
        let all = all.clone();
        let signal = signal.clone();
        let update = update.clone();
        let base = base.clone();
        jobs.push(tokio::spawn(async move {
            let _permit = semaphore
                .acquire_owned()
                .await
                .map_err(|e| HandlerError::new(e.to_string()))?;
            let values = all.clone();
            let notify = update.clone();
            let d = base.clone();
            let callback: AgentToolUpdateCallback = Arc::new(move |partial| {
                if let Some(r) = partial
                    .details
                    .and_then(|d| d["results"].as_array().and_then(|v| v.first()).cloned())
                {
                    values.lock().unwrap()[index] = r;
                    emit_parallel(&values, &notify, &d)?;
                }
                Ok(())
            });
            let r = run_single(&dispatch, &item, None, signal, Some(callback), base.clone()).await?;
            all.lock().unwrap()[index] = r.clone();
            emit_parallel(&all, &update, &base)?;
            Ok::<_, HandlerError>(r)
        }));
    }
    let mut results = Vec::new();
    for job in jobs {
        results.push(job.await.map_err(|e| HandlerError::new(e.to_string()))??);
    }
    let success = results.iter().filter(|r| !failed(r)).count();
    let summaries = JsString::join(
        results
            .iter()
            .map(|r| {
                let status = if failed(r) {
                    let reason = r["stopReason"].as_string().filter(|r| *r != "end");
                    js_format!("failed{}", reason.map(|r| js_format!(" ({})", r)).unwrap_or_default())
                } else {
                    "completed".into()
                };
                let output = result_output(r);
                let output = truncate_output(&output);
                js_format!("### [{}] {}\n\n{}", text(&r["agent"]), status, output)
            })
            .collect::<Vec<_>>(),
        "\n\n---\n\n",
    );
    base["results"] = json!(results);
    Ok(result(
        js_format!("Parallel: {}/{} succeeded\n\n{}", success, tasks.len(), summaries),
        base,
        false,
    ))
}
fn emit_parallel(
    all: &Mutex<Vec<Value>>,
    update: &Option<AgentToolUpdateCallback>,
    base: &Value,
) -> Result<(), pi_agent_core::agent::ToolError> {
    if let Some(update) = update {
        let all = all.lock().unwrap().clone();
        let running = all
            .iter()
            .filter(|r| r["exitCode"] == pi_js_compat::js_json!(-1))
            .count();
        let mut d = base.clone();
        d["results"] = json!(all);
        update(result(
            js_format!(
                "Parallel: {}/{} done, {} running...",
                all.len() - running,
                all.len(),
                running
            ),
            d,
            false,
        ))?;
    }
    Ok(())
}
fn utf8_byte_length(text: &JsString) -> usize {
    char::decode_utf16(text.units().iter().copied())
        .map(|character| character.map_or(3, char::len_utf8))
        .sum()
}

fn truncate_output(s: &JsString) -> JsString {
    const OUTPUT_CAP: usize = 10 * 1024;
    let bytes = utf8_byte_length(s);
    if bytes <= OUTPUT_CAP {
        return s.clone();
    }
    let mut prefix = s.slice(0, OUTPUT_CAP);
    while utf8_byte_length(&prefix) > OUTPUT_CAP {
        prefix = prefix.slice(0, prefix.len() - 1);
    }
    js_format!(
        "{}\n\n[Output truncated: {} bytes omitted. Full output preserved in tool details.]",
        prefix,
        bytes - utf8_byte_length(&prefix)
    )
}

async fn run_single(
    dispatch: &Dispatch,
    item: &Value,
    step: Option<usize>,
    signal: Option<AbortSignal>,
    update: Option<AgentToolUpdateCallback>,
    mut base: Value,
) -> Result<Value, HandlerError> {
    let name = text(&item["agent"]);
    let task = text(&item["task"]);
    let mut r = empty_result(&name, &task, 0);
    if let Some(step) = step {
        r["step"] = json!(step);
    }
    let Some(agent) = dispatch.agents.iter().find(|a| a.name == name) else {
        let names = JsString::join(
            dispatch
                .agents
                .iter()
                .map(|a| js_format!("\"{}\"", a.name))
                .collect::<Vec<_>>(),
            ", ",
        );
        r["exitCode"] = json!(1);
        r["stderr"] = json!(js_format!(
            "Unknown agent: \"{}\". Available agents: {}.",
            name,
            if names.is_empty() {
                JsString::from("none")
            } else {
                names
            }
        ));
        return Ok(r);
    };
    r["agentSource"] = json!(agent.source);
    let model = agent.model.as_ref();
    if let Some(model) = &agent.model {
        r["model"] = json!(model);
    }
    let mut args = vec![
        JsString::from("--mode"),
        "json".into(),
        "-p".into(),
        "--no-session".into(),
    ];
    if let Some(model) = model.filter(|m| !m.is_empty()) {
        args.extend(["--model".into(), model.clone()]);
    }
    if let Some(tools) = &agent.tools {
        args.extend(["--tools".into(), JsString::join(tools, ",")]);
    }
    let prompt = if !agent.system_prompt.trim().is_empty() {
        Some(write_prompt(agent)?)
    } else {
        None
    };
    if let Some((_, path)) = &prompt {
        args.extend([
            "--append-system-prompt".into(),
            JsString::from(path.to_str().expect("prompt path must be UTF-8")),
        ]);
    }
    args.push(js_format!("Task: {}", task));
    let mut command = Command::new(&dispatch.invocation.program);
    command
        .args(dispatch.invocation.prefix.iter().map(encode_node_utf8))
        .args(args.iter().map(encode_node_utf8))
        .current_dir(&encode_node_utf8(&item["cwd"].as_string().unwrap_or(&dispatch.cwd)))
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let Ok(mut child) = command.spawn() else {
        r["exitCode"] = json!(1);
        return Ok(r);
    };
    let stdout = child.stdout.take().expect("piped stdout");
    let mut stderr = child.stderr.take().expect("piped stderr");
    let stderr_text = Arc::new(Mutex::new(JsString::default()));
    let captured = stderr_text.clone();
    let stderr = tokio::spawn(async move {
        let mut bytes = [0; 8192];
        loop {
            let count = stderr.read(&mut bytes).await?;
            if count == 0 {
                return Ok::<_, std::io::Error>(());
            }
            captured
                .lock()
                .unwrap()
                .push_str(String::from_utf8_lossy(&bytes[..count]).as_ref());
        }
    });
    let mut reader = BufReader::new(stdout);
    let mut bytes = Vec::new();
    let cancel = signal.unwrap_or_default();
    let mut aborted = false;
    loop {
        bytes.clear();
        let read = tokio::select! {biased;
         _=cancel.wait(),if !aborted=>{aborted=true;if let Some(pid)=child.id().and_then(|p|rustix::process::Pid::from_raw(p as i32)){let _=rustix::process::kill_process(pid,rustix::process::Signal::TERM);}continue;},
         read=reader.read_until(b'\n',&mut bytes)=>read.map_err(|e|HandlerError::new(e.to_string()))?,
        };
        if read == 0 {
            break;
        }
        if let Ok(event) = pi_js_compat::js_json::parse_lossless(String::from_utf8_lossy(&bytes).as_ref()) {
            r["stderr"] = json!(stderr_text.lock().unwrap().clone());
            apply_event(&mut r, event, &mut base, &update);
        }
    }
    let status = child.wait().await.map_err(|e| HandlerError::new(e.to_string()))?;
    r["exitCode"] = json!(status.code().unwrap_or(0));
    stderr
        .await
        .map_err(|e| HandlerError::new(e.to_string()))?
        .map_err(|e| HandlerError::new(e.to_string()))?;
    r["stderr"] = json!(stderr_text.lock().unwrap().clone());
    drop(prompt);
    if aborted {
        return Err(HandlerError::new("Subagent was aborted"));
    }
    Ok(r)
}
fn write_prompt(agent: &AgentConfig) -> Result<(tempfile::TempDir, PathBuf), HandlerError> {
    let dir = tempfile::Builder::new()
        .prefix("pi-subagent-")
        .tempdir()
        .map_err(|e| HandlerError::new(e.to_string()))?;
    let safe = pi_js_compat::js_regex::JsRegex::new(r"[^a-zA-Z0-9_.-]+")
        .expect("valid filename sanitizer regex")
        .replace_all(&agent.name, "_");
    let path = dir.path().join(&encode_node_utf8(&js_format!("prompt-{}.md", safe)));
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .mode(0o600)
        .open(&path)
        .map_err(|e| HandlerError::new(e.to_string()))?;
    file.write_all(encode_node_utf8(&agent.system_prompt).as_bytes())
        .map_err(|e| HandlerError::new(e.to_string()))?;
    Ok((dir, path))
}
fn apply_event(r: &mut Value, event: Value, base: &mut Value, update: &Option<AgentToolUpdateCallback>) {
    if event["message"].is_null()
        || !(text(&event["type"]) == "message_end" || text(&event["type"]) == "tool_result_end")
    {
        return;
    }
    let message = event["message"].clone();
    r["messages"]
        .as_array_mut()
        .expect("messages array")
        .push(message.clone());
    if event["type"] == pi_js_compat::js_json!("message_end") && message["role"] == pi_js_compat::js_json!("assistant")
    {
        r["usage"]["turns"] = json!(r["usage"]["turns"].as_u64().unwrap_or(0) + 1);
        for key in ["input", "output", "cacheRead", "cacheWrite"] {
            let total = r["usage"][key].as_f64().unwrap_or(0.0) + message["usage"][key].as_f64().unwrap_or(0.0);
            r["usage"][key] = JsValue::from(pi_js_compat::number::js_number_value(total));
        }
        r["usage"]["cost"] = JsValue::from(pi_js_compat::number::js_number_value(
            r["usage"]["cost"].as_f64().unwrap_or(0.0) + message["usage"]["cost"]["total"].as_f64().unwrap_or(0.0),
        ));
        r["usage"]["contextTokens"] = JsValue::from(pi_js_compat::number::js_number_value(
            message["usage"]["totalTokens"].as_f64().unwrap_or(0.0),
        ));
        for key in ["stopReason", "errorMessage"] {
            if message[key].as_string().is_some_and(|s| !s.is_empty()) {
                r[key] = message[key].clone();
            }
        }
        if r["model"].as_string().is_none_or(|s| s.is_empty()) && message["model"].is_string() {
            r["model"] = message["model"].clone();
        }
    }
    if let Some(update) = update {
        base["results"] = json!([r]);
        let s = output(&r["messages"]);
        update(result(
            if s.is_empty() { "(running...)".into() } else { s },
            base.clone(),
            false,
        ))
        .unwrap_or_else(|error| {
            pi_coding_agent::utils::uncaught::throw_uncaught(pi_coding_agent::utils::uncaught::ERROR, error.message)
        });
    }
}
