//! Opt-in native `notify.ts`; no desktop notification services are loaded.
use pi_coding_agent::core::extensions::*;
use std::{
    io::{Write, stdout},
    process::Stdio,
    sync::{Arc, Mutex},
};

fn windows_toast_script(title: &str, body: &str) -> String {
    let kind = "Windows.UI.Notifications";
    let manager = format!("[{kind}.ToastNotificationManager, {kind}, ContentType = WindowsRuntime]");
    let template = format!("[{kind}.ToastTemplateType]::ToastText01");
    let toast = format!("[{kind}.ToastNotification]::new($xml)");
    [
        format!("{manager} > $null"),
        format!("$xml = [{kind}.ToastNotificationManager]::GetTemplateContent({template})"),
        format!("$xml.GetElementsByTagName('text')[0].AppendChild($xml.CreateTextNode('{body}')) > $null"),
        format!("[{kind}.ToastNotificationManager]::CreateToastNotifier('{title}').Show({toast})"),
    ]
    .join("; ")
}
fn spawn_windows_notification(title: &str, body: &str) {
    // Source execFile has no callback: spawn/wait/exit failures are unobserved.
    // Preserve that classified source discrepancy, while reaping native children.
    let Ok(mut child) = tokio::process::Command::new("powershell.exe")
        .args(["-NoProfile", "-Command", &windows_toast_script(title, body)])
        .env_clear()
        .envs(pi_js_compat::process_env::vars())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    else {
        return;
    };
    tokio::spawn(async move { child.wait().await });
}
pub fn notify_extension(pi: &ExtensionApi) -> Result<(), HandlerError> {
    notify_with_output(pi, stdout())
}
/// Embedding hosts can supply their terminal writer, as in model_status.rs.
pub fn notify_with_output(pi: &ExtensionApi, output: impl Write + Send + 'static) -> Result<(), HandlerError> {
    let output = Arc::new(Mutex::new(output));
    pi.on_agent_end(move |_event, _ctx| {
        let output = output.clone();
        Box::pin(async move {
            if pi_js_compat::process_env::get("WT_SESSION").is_some_and(|value| !value.is_empty()) {
                spawn_windows_notification("Pi", "Ready for input");
                return Ok(None);
            }
            let sequences = if pi_js_compat::process_env::get("KITTY_WINDOW_ID").is_some_and(|value| !value.is_empty())
            {
                vec!["\x1b]99;i=1:d=0;Pi\x1b\\", "\x1b]99;i=1:p=body;Ready for input\x1b\\"]
            } else {
                vec!["\x1b]777;notify;Pi;Ready for input\x07"]
            };
            let mut output = output.lock().map_err(|error| HandlerError::new(error.to_string()))?;
            for sequence in sequences {
                output
                    .write_all(sequence.as_bytes())
                    .map_err(|error| HandlerError::new(error.to_string()))?;
                // OSC sequences have no newline; don't leave them in Stdout's buffer.
                output.flush().map_err(|error| HandlerError::new(error.to_string()))?;
            }
            Ok(None)
        }) as BoxFuture<'static, Result<Option<()>, HandlerError>>
    })
    .map_err(HandlerError::new)
}
