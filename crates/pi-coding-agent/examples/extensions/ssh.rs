//! Opt-in port of `examples/extensions/ssh.ts` at 532f7d12.
//! Explicitly register `ssh_extension`; never autoloaded.
use pi_agent_core::{AgentTool, ToolError};
use pi_ai::request::BoxFuture;
use pi_coding_agent::{
    core::{
        extensions::*,
        tools::{
            bash::{BashExecOptions, BashExecResult, BashOperations, BashToolOptions, create_bash_tool},
            edit::{EditOperations, EditToolOptions, create_edit_tool},
            read::{ReadOperations, ReadToolOptions, create_read_tool},
            tool_definition_wrapper::create_tool_definition_from_agent_tool,
            write::{WriteOperations, WriteToolOptions, create_write_tool},
        },
    },
    modes::interactive::theme::ThemeColor,
    utils::child_process::node_spawn_error_message,
};
use pi_js_compat::base64::buffer_to_base64;
use pi_js_compat::js_format;
use pi_js_compat::js_json::JsString;
use pi_js_compat::{number::js_number_to_string, string::js_trim};
use std::{
    io,
    process::Stdio,
    sync::{Arc, Mutex},
};
use tokio::{
    io::{AsyncRead, AsyncReadExt},
    process::Command,
};

#[derive(Clone)]
struct Remote {
    host: String,
    cwd: JsString,
    local: JsString,
}
impl Remote {
    fn path(&self, path: impl Into<pi_js_compat::js_json::JsString>) -> pi_js_compat::js_json::JsString {
        pi_js_compat::string::replace(path, &self.local, &self.cwd)
    }
    fn quote_path(&self, path: &JsString) -> String {
        quote(self.path(path))
    }
    async fn run(&self, command: String) -> io::Result<Vec<u8>> {
        let output = command_for(&self.host, &JsString::from(&command))
            .output()
            .await
            .map_err(|error| {
                io::Error::other(
                    node_spawn_error_message("spawn", &pi_js_compat::js_json::JsString::from("ssh"), &error)
                        .to_utf8()
                        .expect("fixed ssh spawn error contains Unicode scalar text"),
                )
            })?;
        if !output.status.success() {
            let code = output.status.code().map_or_else(|| "null".into(), |n| n.to_string());
            return Err(io::Error::other(format!(
                "SSH failed ({code}): {}",
                String::from_utf8_lossy(&output.stderr)
            )));
        }
        Ok(output.stdout)
    }
}
fn quote(value: impl Into<pi_js_compat::js_json::JsString>) -> String {
    // JSON.stringify produces a scalar wire literal even for unmatched input units.
    pi_js_compat::js_json::stringify_lossless(&pi_js_compat::js_json::JsValue::String(value.into()))
}
fn command_for(host: &str, command: &JsString) -> Command {
    let mut child = Command::new("ssh");
    child
        .args([host, &pi_js_compat::js_json::encode_node_utf8(command)])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    child
}
impl ReadOperations for Remote {
    fn read_file<'a>(&'a self, path: &'a JsString) -> BoxFuture<'a, io::Result<Vec<u8>>> {
        Box::pin(async move { self.run(format!("cat {}", self.quote_path(path))).await })
    }
    fn access<'a>(&'a self, path: &'a JsString) -> BoxFuture<'a, io::Result<()>> {
        Box::pin(async move { self.run(format!("test -r {}", self.quote_path(path))).await.map(|_| ()) })
    }
    fn detect_image_mime_type<'a>(&'a self, path: &'a JsString) -> BoxFuture<'a, io::Result<Option<String>>> {
        Box::pin(async move {
            let Ok(bytes) = self.run(format!("file --mime-type -b {}", self.quote_path(path))).await else {
                return Ok(None);
            };
            let mime = js_trim(&String::from_utf8_lossy(&bytes)).to_owned();
            Ok(["image/jpeg", "image/png", "image/gif", "image/webp"]
                .contains(&mime.as_str())
                .then_some(mime))
        })
    }
}
impl WriteOperations for Remote {
    fn mkdir<'a>(&'a self, path: &'a JsString) -> BoxFuture<'a, io::Result<()>> {
        Box::pin(async move {
            self.run(format!("mkdir -p {}", self.quote_path(path)))
                .await
                .map(|_| ())
        })
    }
    fn write_file<'a>(&'a self, path: &'a JsString, content: &'a JsString) -> BoxFuture<'a, io::Result<()>> {
        Box::pin(async move {
            self.run(format!(
                "echo {} | base64 -d > {}",
                quote(buffer_to_base64(
                    pi_js_compat::js_json::encode_node_utf8(content).as_bytes()
                )),
                self.quote_path(path)
            ))
            .await
            .map(|_| ())
        })
    }
}
impl EditOperations for Remote {
    fn read_file<'a>(&'a self, path: &'a JsString) -> BoxFuture<'a, io::Result<Vec<u8>>> {
        ReadOperations::read_file(self, path)
    }
    fn access<'a>(&'a self, path: &'a JsString) -> BoxFuture<'a, io::Result<()>> {
        ReadOperations::access(self, path)
    }
    fn write_file<'a>(&'a self, path: &'a JsString, content: &'a JsString) -> BoxFuture<'a, io::Result<()>> {
        WriteOperations::write_file(self, path, content)
    }
}
async fn forward(
    mut pipe: impl AsyncRead + Unpin,
    on_data: pi_coding_agent::core::tools::bash::BashDataCallback,
) -> io::Result<()> {
    let mut buffer = [0; 8192];
    loop {
        let n = pipe.read(&mut buffer).await?;
        if n == 0 {
            return Ok(());
        }
        on_data(&buffer[..n]).unwrap_or_else(|error| pi_coding_agent::utils::uncaught::throw_callback_uncaught(error));
    }
}
impl BashOperations for Remote {
    fn exec<'a>(
        &'a self,
        command: &'a JsString,
        cwd: &'a JsString,
        options: BashExecOptions,
    ) -> BoxFuture<'a, Result<BashExecResult, ToolError>> {
        Box::pin(async move {
            let command = js_format!("cd {} && {}", self.quote_path(cwd), command);
            let mut child = command_for(&self.host, &command).spawn().map_err(|error| {
                ToolError::new(
                    node_spawn_error_message("spawn", &pi_js_compat::js_json::JsString::from("ssh"), &error)
                        .to_utf8()
                        .expect("fixed ssh spawn error contains Unicode scalar text"),
                )
            })?;
            let stdout = child.stdout.take().expect("piped stdout");
            let stderr = child.stderr.take().expect("piped stderr");
            let output = async {
                tokio::try_join!(
                    forward(stdout, options.on_data.clone()),
                    forward(stderr, options.on_data.clone())
                )
            };
            // AbortSignal listeners do not replay an abort that already happened.
            let wait = wait_for_remote(&mut child, options.signal.clone(), options.timeout);
            let ((status, timed_out), _) = tokio::try_join!(wait, output).map_err(|e| ToolError::new(e.to_string()))?;
            finish_remote_bash(status, timed_out, &options)
        })
    }
}
fn finish_remote_bash(
    status: std::process::ExitStatus,
    timed_out: bool,
    options: &BashExecOptions,
) -> Result<BashExecResult, ToolError> {
    if options
        .signal
        .as_ref()
        .is_some_and(pi_js_compat::abort::AbortSignal::aborted)
    {
        return Err(ToolError::new("aborted"));
    }
    if timed_out {
        return Err(ToolError::new(format!(
            "timeout:{}",
            js_number_to_string(options.timeout.expect("timer timeout"))
        )));
    }
    Ok(BashExecResult {
        exit_code: status.code().map(f64::from),
        detached: None,
    })
}

async fn wait_for_remote(
    child: &mut tokio::process::Child,
    signal: Option<pi_js_compat::abort::AbortSignal>,
    timeout: Option<f64>,
) -> io::Result<(std::process::ExitStatus, bool)> {
    let signal = signal.filter(|signal| !signal.aborted());
    let aborted = async {
        match signal {
            Some(signal) => signal.wait().await,
            None => std::future::pending().await,
        }
    };
    let timer = async {
        match timeout.filter(|seconds| *seconds != 0.0 && !seconds.is_nan()) {
            Some(seconds) => {
                let delay_ms = pi_js_compat::node_timers::normalize_timeout_delay(seconds * 1000.0);
                tokio::time::sleep(std::time::Duration::from_secs_f64(delay_ms / 1000.0)).await;
            }
            None => std::future::pending().await,
        }
    };
    let mut timed_out = false;
    let status = tokio::select! {
        status = child.wait() => status,
        _ = aborted => { terminate(child)?; child.wait().await },
        _ = timer => { timed_out = true; terminate(child)?; child.wait().await },
    }?;
    Ok((status, timed_out))
}
// Node ChildProcess.kill() sends SIGTERM, not Tokio's SIGKILL.
fn terminate(child: &tokio::process::Child) -> io::Result<()> {
    if let Some(pid) = child.id().and_then(|id| rustix::process::Pid::from_raw(id as i32)) {
        rustix::process::kill_process(pid, rustix::process::Signal::TERM).map_err(io::Error::from)?;
    }
    Ok(())
}
fn remote_tools(local: &str, remote: Option<Remote>) -> Vec<Arc<dyn AgentTool>> {
    let operations = remote.map(Arc::new);
    vec![
        create_read_tool(
            local,
            ReadToolOptions {
                operations: operations.clone().map(|r| r as Arc<dyn ReadOperations>),
                ..Default::default()
            },
        ),
        create_write_tool(
            local,
            WriteToolOptions {
                operations: operations.clone().map(|r| r as Arc<dyn WriteOperations>),
            },
        ),
        create_edit_tool(
            local,
            EditToolOptions {
                operations: operations.clone().map(|r| r as Arc<dyn EditOperations>),
            },
        ),
        create_bash_tool(
            local,
            BashToolOptions {
                operations: operations.map(|r| r as Arc<dyn BashOperations>),
                ..Default::default()
            },
        ),
    ]
}
/// TS `ssh.ts:114-220`: flag, four lazy overrides, session/user-bash/prompt hooks.
pub fn ssh_extension(pi: &ExtensionApi) -> Result<(), HandlerError> {
    pi.register_flag(
        "ssh",
        Some("SSH remote: user@host or user@host:/path".into()),
        FlagType::String,
        None,
    )?;
    let local = pi_js_compat::process_cwd::cwd()
        .map_err(HandlerError::from)?
        .to_utf8()
        .expect("Node OS-byte decoder returns Unicode scalars");
    let resolved: Arc<Mutex<Option<Remote>>> = Arc::default();
    register_tools(pi, &local, &resolved)?;
    register_session_start(pi, local, resolved.clone())?;
    register_user_bash(pi, resolved.clone())?;
    register_prompt(pi, resolved)?;
    Ok(())
}

type RemoteState = Arc<Mutex<Option<Remote>>>;

fn register_tools(pi: &ExtensionApi, local: &str, resolved: &RemoteState) -> Result<(), HandlerError> {
    for tool in remote_tools(local, None) {
        let mut definition = create_tool_definition_from_agent_tool(tool.clone());
        let state = resolved.clone();
        let local = local.to_owned();
        definition.execute = Arc::new(move |id, params, signal, update, _ctx| {
            let remote = state.lock().expect("SSH state").clone();
            let tool = match remote {
                Some(remote) => remote_tools(&local, Some(remote))
                    .into_iter()
                    .find(|t| t.name() == tool.name())
                    .expect("registered tool"),
                None => tool.clone(),
            };
            Box::pin(async move {
                tool.execute(id, params, signal, update.unwrap_or_else(|| Arc::new(|_| Ok(()))), None)
                    .await
                    .map_err(|e| HandlerError::new(e.message))
            })
        });
        pi.register_tool(definition)?;
    }
    Ok(())
}

async fn fetch_remote_config(arg: &str, local: String) -> Result<Remote, HandlerError> {
    let mut parts = arg.split(':');
    let host = parts.next().unwrap_or_default().to_owned();
    let path = parts.next();
    let mut remote = Remote {
        host,
        cwd: JsString::from(path.unwrap_or_default().to_owned()),
        local: JsString::from(local),
    };
    if path.is_none() {
        let bytes = remote
            .run("pwd".into())
            .await
            .map_err(|error| HandlerError::new(error.to_string()))?;
        remote.cwd = JsString::from(js_trim(&String::from_utf8_lossy(&bytes)).to_owned());
    }
    Ok(remote)
}

fn register_session_start(pi: &ExtensionApi, local: String, state: RemoteState) -> Result<(), HandlerError> {
    let api = pi.clone();
    pi.on_session_start(move |_: SessionStartEvent, ctx: ExtensionContext| {
        let api = api.clone();
        let state = state.clone();
        let local = local.clone();
        Box::pin(async move {
            let Some(FlagValue::String(arg)) = api.get_flag("ssh")? else {
                return Ok(None);
            };
            if arg.is_empty() {
                return Ok(None);
            }
            let remote = fetch_remote_config(&arg, local).await?;
            let label = js_format!("{}:{}", remote.host, remote.cwd);
            *state.lock().expect("SSH state") = Some(remote);
            let ui = ctx.ui()?;
            ui.set_status(
                &JsString::from("ssh"),
                Some(ExtensionStatus::Text(
                    ui.theme()?.fg(ThemeColor::Accent, &js_format!("SSH: {}", label)),
                )),
            )?;
            ui.notify(&js_format!("SSH mode: {}", label), Some(NotifyType::Info))?;
            Ok(None)
        }) as BoxFuture<'static, Result<Option<()>, HandlerError>>
    })?;
    Ok(())
}

fn register_user_bash(pi: &ExtensionApi, state: RemoteState) -> Result<(), HandlerError> {
    pi.on_user_bash(move |_: UserBashEvent, _: ExtensionContext| {
        let remote = state.lock().expect("SSH state").clone();
        Box::pin(async move {
            Ok(remote.map(|r| UserBashEventResult {
                operations: Some(Arc::new(r)),
                result: None,
            }))
        }) as BoxFuture<'static, Result<Option<UserBashEventResult>, HandlerError>>
    })?;
    Ok(())
}

fn register_prompt(pi: &ExtensionApi, resolved: RemoteState) -> Result<(), HandlerError> {
    pi.on_before_agent_start(move |event: BeforeAgentStartEvent, _: ExtensionContext| {
        let remote = resolved.lock().expect("SSH state").clone();
        Box::pin(async move {
            Ok(remote.map(|r| BeforeAgentStartEventResult {
                system_prompt: Some(pi_js_compat::string::replace(
                    &event.system_prompt,
                    &js_format!("Current working directory: {}", r.local),
                    &js_format!("Current working directory: {} (via SSH: {})", r.cwd, r.host),
                )),
                ..Default::default()
            }))
        }) as BoxFuture<'static, Result<Option<BeforeAgentStartEventResult>, HandlerError>>
    })?;
    Ok(())
}
