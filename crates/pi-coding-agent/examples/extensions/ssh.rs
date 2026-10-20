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
    cwd: String,
    local: String,
}
impl Remote {
    fn path(&self, path: &str) -> String {
        pi_js_compat::string::replace(path, &self.local, &self.cwd)
    }
    fn quote_path(&self, path: &str) -> String {
        quote(&self.path(path))
    }
    async fn run(&self, command: String) -> io::Result<Vec<u8>> {
        let output = command_for(&self.host, &command)
            .output()
            .await
            .map_err(|error| io::Error::other(node_spawn_error_message("spawn", "ssh", &error)))?;
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
fn quote(value: &str) -> String {
    pi_js_compat::js_json::stringify(&serde_json::Value::String(value.into()))
}
fn command_for(host: &str, command: &str) -> Command {
    let mut child = Command::new("ssh");
    child
        .args([host, command])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    child
}
impl ReadOperations for Remote {
    fn read_file<'a>(&'a self, path: &'a str) -> BoxFuture<'a, io::Result<Vec<u8>>> {
        Box::pin(async move { self.run(format!("cat {}", self.quote_path(path))).await })
    }
    fn access<'a>(&'a self, path: &'a str) -> BoxFuture<'a, io::Result<()>> {
        Box::pin(async move { self.run(format!("test -r {}", self.quote_path(path))).await.map(|_| ()) })
    }
    fn detect_image_mime_type<'a>(&'a self, path: &'a str) -> BoxFuture<'a, io::Result<Option<String>>> {
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
    fn mkdir<'a>(&'a self, path: &'a str) -> BoxFuture<'a, io::Result<()>> {
        Box::pin(async move {
            self.run(format!("mkdir -p {}", self.quote_path(path)))
                .await
                .map(|_| ())
        })
    }
    fn write_file<'a>(&'a self, path: &'a str, content: &'a str) -> BoxFuture<'a, io::Result<()>> {
        Box::pin(async move {
            self.run(format!(
                "echo {} | base64 -d > {}",
                quote(&buffer_to_base64(content.as_bytes())),
                self.quote_path(path)
            ))
            .await
            .map(|_| ())
        })
    }
}
impl EditOperations for Remote {
    fn read_file<'a>(&'a self, path: &'a str) -> BoxFuture<'a, io::Result<Vec<u8>>> {
        ReadOperations::read_file(self, path)
    }
    fn access<'a>(&'a self, path: &'a str) -> BoxFuture<'a, io::Result<()>> {
        ReadOperations::access(self, path)
    }
    fn write_file<'a>(&'a self, path: &'a str, content: &'a str) -> BoxFuture<'a, io::Result<()>> {
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
        on_data(&buffer[..n]);
    }
}
impl BashOperations for Remote {
    fn exec<'a>(
        &'a self,
        command: &'a str,
        cwd: &'a str,
        options: BashExecOptions,
    ) -> BoxFuture<'a, Result<BashExecResult, ToolError>> {
        Box::pin(async move {
            let command = format!("cd {} && {command}", self.quote_path(cwd));
            let mut child = command_for(&self.host, &command)
                .spawn()
                .map_err(|error| ToolError::new(node_spawn_error_message("spawn", "ssh", &error)))?;
            let stdout = child.stdout.take().expect("piped stdout");
            let stderr = child.stderr.take().expect("piped stderr");
            let output = async {
                tokio::try_join!(
                    forward(stdout, options.on_data.clone()),
                    forward(stderr, options.on_data.clone())
                )
            };
            let signal = options.signal.clone();
            let aborted = async {
                match signal {
                    Some(signal) => signal.cancelled().await,
                    None => std::future::pending().await,
                }
            };
            let timer = async {
                match options.timeout.filter(|n| *n != 0.0) {
                    Some(seconds) => tokio::time::sleep(std::time::Duration::from_secs_f64(seconds.max(0.001))).await,
                    None => std::future::pending().await,
                }
            };
            let wait = async {
                let mut timed_out = false;
                let status = tokio::select! {
                    status = child.wait() => status,
                    () = aborted => { terminate(&child)?; child.wait().await },
                    () = timer => { timed_out = true; terminate(&child)?; child.wait().await },
                }?;
                Ok::<_, io::Error>((status, timed_out))
            };
            let ((status, timed_out), _) = tokio::try_join!(wait, output).map_err(|e| ToolError::new(e.to_string()))?;
            if options
                .signal
                .as_ref()
                .is_some_and(tokio_util::sync::CancellationToken::is_cancelled)
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
                exit_code: status.code(),
                detached: None,
            })
        })
    }
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
/// TS `ssh.ts:120-238`: flag, four lazy overrides, session/user-bash/prompt hooks.
pub fn ssh_extension(pi: &ExtensionApi) -> Result<(), HandlerError> {
    pi.register_flag(
        "ssh",
        Some("SSH remote: user@host or user@host:/path".into()),
        FlagType::String,
        None,
    )?;
    let local = std::env::current_dir()
        .map_err(|e| HandlerError::new(e.to_string()))?
        .to_string_lossy()
        .into_owned();
    let resolved: Arc<Mutex<Option<Remote>>> = Arc::default();
    for tool in remote_tools(&local, None) {
        let mut definition = create_tool_definition_from_agent_tool(tool.clone());
        let state = resolved.clone();
        let local = local.clone();
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
                tool.execute(id, params, signal, update.unwrap_or_else(|| Arc::new(|_| {})), None)
                    .await
                    .map_err(|e| HandlerError::new(e.message))
            })
        });
        pi.register_tool(definition)?;
    }
    let api = pi.clone();
    let state = resolved.clone();
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
            let mut parts = arg.split(':');
            let host = parts.next().unwrap_or_default().to_owned();
            let cwd = match parts.next() {
                Some(path) => path.to_owned(),
                None => {
                    let bytes = Remote {
                        host: host.clone(),
                        cwd: String::new(),
                        local: local.clone(),
                    }
                    .run("pwd".into())
                    .await
                    .map_err(|e| HandlerError::new(e.to_string()))?;
                    js_trim(&String::from_utf8_lossy(&bytes)).to_owned()
                }
            };
            let label = format!("{host}:{cwd}");
            *state.lock().expect("SSH state") = Some(Remote { host, cwd, local });
            let ui = ctx.ui()?;
            ui.set_status(
                "ssh",
                Some(&ui.theme().fg(ThemeColor::Accent, &format!("SSH: {label}"))),
            );
            ui.notify(&format!("SSH mode: {label}"), Some(NotifyType::Info));
            Ok(None)
        }) as BoxFuture<'static, Result<Option<()>, HandlerError>>
    })?;
    let state = resolved.clone();
    pi.on_user_bash(move |_: UserBashEvent, _: ExtensionContext| {
        let remote = state.lock().expect("SSH state").clone();
        Box::pin(async move {
            Ok(remote.map(|r| UserBashEventResult {
                operations: Some(Arc::new(r)),
                result: None,
            }))
        }) as BoxFuture<'static, Result<Option<UserBashEventResult>, HandlerError>>
    })?;
    pi.on_before_agent_start(move |event: BeforeAgentStartEvent, _: ExtensionContext| {
        let remote = resolved.lock().expect("SSH state").clone();
        Box::pin(async move {
            Ok(remote.map(|r| BeforeAgentStartEventResult {
                system_prompt: Some(pi_js_compat::string::replace(
                    &event.system_prompt,
                    &format!("Current working directory: {}", r.local),
                    &format!("Current working directory: {} (via SSH: {})", r.cwd, r.host),
                )),
                ..Default::default()
            }))
        }) as BoxFuture<'static, Result<Option<BeforeAgentStartEventResult>, HandlerError>>
    })?;
    Ok(())
}
