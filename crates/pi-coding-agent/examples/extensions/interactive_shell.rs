//! Explicitly registered native interactive-shell example; never autoloaded.
use pi_coding_agent::core::extensions::*;
use pi_js_compat::js_json::JsString;
use pi_js_compat::js_json::JsValue;
use pi_js_compat::string::js_trim;
use serde_json::json;
use std::{
    io::Write,
    process::{Command, Stdio},
    sync::{Arc, Mutex},
};
type Environment = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;
const DEFAULT_COMMANDS: &[&str] = &[
    "vim",
    "nvim",
    "vi",
    "nano",
    "emacs",
    "pico",
    "micro",
    "helix",
    "hx",
    "kak",
    "less",
    "more",
    "most",
    "git commit",
    "git rebase",
    "git merge",
    "git cherry-pick",
    "git revert",
    "git add -p",
    "git add --patch",
    "git add -i",
    "git add --interactive",
    "git stash -p",
    "git stash --patch",
    "git reset -p",
    "git reset --patch",
    "git checkout -p",
    "git checkout --patch",
    "git difftool",
    "git mergetool",
    "htop",
    "top",
    "btop",
    "glances",
    "ranger",
    "nnn",
    "lf",
    "mc",
    "vifm",
    "tig",
    "lazygit",
    "gitui",
    "fzf",
    "sk",
    "ssh",
    "telnet",
    "mosh",
    "psql",
    "mysql",
    "sqlite3",
    "mongosh",
    "redis-cli",
    "kubectl edit",
    "kubectl exec -it",
    "docker exec -it",
    "docker run -it",
    "tmux",
    "screen",
    "ncdu",
];
fn is_interactive(command: &JsString, env: &Environment) -> bool {
    let command = command.trim().to_lowercase();
    let additional = env("INTERACTIVE_COMMANDS").unwrap_or_default();
    let excluded: Vec<_> = env("INTERACTIVE_EXCLUDE")
        .unwrap_or_default()
        .split(',')
        .map(|s| js_trim(s).to_lowercase())
        .collect();
    let commands = DEFAULT_COMMANDS
        .iter()
        .copied()
        .chain(additional.split(',').map(js_trim).filter(|s| !s.is_empty()));
    for cmd in commands {
        let cmd = cmd.to_lowercase();
        if excluded.contains(&cmd) {
            continue;
        }
        if command == cmd || command.starts_with(format!("{cmd} ")) || command.starts_with(format!("{cmd}\t")) {
            return true;
        }
        if command.contains('|') {
            let tail = command.split('|').last().expect("split yields a final command").trim();
            if tail == cmd || tail.starts_with(format!("{cmd} ")) {
                return true;
            }
        }
    }
    false
}
pub fn interactive_shell_extension(api: &ExtensionApi) -> Result<(), HandlerError> {
    interactive_shell_extension_with_environment(api, Arc::new(|key| std::env::var(key).ok()))
}
pub(crate) fn interactive_shell_extension_with_environment(
    api: &ExtensionApi,
    env: Environment,
) -> Result<(), HandlerError> {
    api.on_user_bash(move |event:UserBashEvent,ctx:ExtensionContext|->BoxFuture<'static,Result<Option<UserBashEventResult>,HandlerError>> {
        let env=env.clone();
        Box::pin(async move {
            let force=event.command.starts_with("i ") || event.command.starts_with("i\t");
            let command=if force {event.command.slice(2, event.command.len()).trim()} else {event.command};
            if !force && !is_interactive(&command,&env) {return Ok(None);}
            if ctx.mode()? != ExtensionMode::Tui {return Ok(Some(UserBashEventResult {operations:None,result:Some(JsValue::from(json!({"output":"(interactive commands require TUI)","exitCode":1,"cancelled":false,"truncated":false})))}));}
            let error=Arc::new(Mutex::new(None));let captured=error.clone();
            let result=ctx.ui()?.custom(Arc::new(move |context,done| {
                match run_inherited(&context.tui,&command,&env) {
                    Ok(status)=>done.call(Some(JsValue::from(json!(status)))),
                    Err(error)=>{*captured.lock().unwrap()=Some(error);done.call(None);}
                }
                pi_coding_agent::core::extensions::ready_custom_component(Box::new(Empty))
            }),None).await?;
            if let Some(error)=error.lock().unwrap().take() {return Err(HandlerError::from(error));}
            let code=result.as_ref().and_then(|v|v.as_f64().map(|code| code as i64));
            let output=if code==Some(0) {"(interactive command completed successfully)".into()} else {format!("(interactive command exited with code {})",code.map_or("null".into(),|c|c.to_string()))};
            Ok(Some(UserBashEventResult {operations:None,result:Some(JsValue::from(json!({"output":output,"exitCode":code.unwrap_or(1),"cancelled":false,"truncated":false})))}))
        })
    }).map_err(HandlerError::new)
}
fn run_inherited(
    tui: &ExtensionUiTui,
    command: &JsString,
    env: &Environment,
) -> Result<Option<i32>, pi_js_compat::callback_error::CallbackError> {
    tui.stop()?;
    let mut stdout = std::io::stdout();
    stdout
        .write_all(b"\x1b[2J\x1b[H")
        .and_then(|()| stdout.flush())
        .map_err(|e| e.to_string())?;
    let shell = env("SHELL")
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "/bin/sh".into());
    // spawnSync returns status=null on spawn failure or signal; its `error` is
    // deliberately ignored by the pinned example (interactive-shell.ts:165-174).
    let status = Command::new(shell)
        .args(["-c", &pi_js_compat::js_json::encode_node_utf8(command)])
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status();
    let status = match status {
        Ok(status) => status.code(),
        Err(_) => None,
    };
    tui.start()?;
    tui.request_render_force();
    Ok(status)
}
struct Empty;
impl ExtensionUiComponent for Empty {
    fn render(&self, _: usize) -> Vec<pi_js_compat::js_json::JsString> {
        Vec::new()
    }
}
