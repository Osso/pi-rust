//! Plain-text clipboard access with OSC 52 fallback.
//!
//! Direct clipboard writes are preferred; OSC 52 is emitted afterwards only for
//! remote sessions or when every local mechanism failed. On Linux the native
//! addon is skipped: its X11 backend does not retain selection ownership after
//! `set_text`, so the platform tools (`wl-copy`, `xclip`, `xsel`) are used.

use std::io::Write;
use std::process::Stdio;
use std::time::Duration;

use pi_js_compat::base64::buffer_to_base64;
use pi_js_compat::js_json::{JsString, encode_node_utf8};
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

use crate::utils::clipboard_image::{ClipboardEnvironment, is_wayland_session};
use crate::utils::clipboard_native::{ClipboardError, ClipboardModule};

const MAX_OSC52_ENCODED_LENGTH: usize = 100_000;
const COPY_TIMEOUT: Duration = Duration::from_millis(5000);
const READ_TIMEOUT: Duration = Duration::from_millis(5000);
const READ_MAX_BUFFER_BYTES: usize = 50 * 1024 * 1024;

/// Whether the session runs over SSH or Mosh.
pub fn is_remote_session(environment: &ClipboardEnvironment) -> bool {
    ["SSH_CONNECTION", "SSH_CLIENT", "MOSH_CONNECTION"]
        .iter()
        .any(|name| environment.variable(name).is_some_and(|value| !value.is_empty()))
}

/// Writes the text as an OSC 52 clipboard sequence; false when too large.
pub fn emit_osc52(text: &str) -> bool {
    let encoded = buffer_to_base64(text.as_bytes());
    if encoded.len() > MAX_OSC52_ENCODED_LENGTH {
        return false;
    }

    let mut stdout = std::io::stdout().lock();
    let _ = write!(stdout, "\u{1b}]52;c;{encoded}\u{7}");
    let _ = stdout.flush();
    true
}

/// Reads plain text from the system clipboard.
pub async fn read_clipboard_text(
    environment: &ClipboardEnvironment,
    native: Option<&dyn ClipboardModule>,
) -> Option<String> {
    if environment.platform() == "linux"
        && is_wayland_session(environment)
        && environment
            .variable("WAYLAND_DISPLAY")
            .is_some_and(|value| !value.is_empty())
        && let Some(text) = read_wayland_clipboard_text().await
    {
        return (!text.is_empty()).then_some(text);
    }

    let native = native?;
    match native.get_text() {
        Ok(text) => (!text.is_empty()).then_some(text),
        Err(_) => None,
    }
}

/// Copies text to the clipboard, throwing when no mechanism succeeds.
///
/// Every sink (native addon, child stdin, OSC 52 `Buffer.from`) receives the
/// Node UTF-8 encoding, so unpaired surrogates become U+FFFD once here.
pub async fn copy_to_clipboard(
    text: &JsString,
    environment: &ClipboardEnvironment,
    native: Option<&dyn ClipboardModule>,
) -> Result<(), ClipboardError> {
    let text = encode_node_utf8(text);
    let text = text.as_str();
    let platform = environment.platform();
    let mut copied = false;

    if platform != "linux"
        && let Some(native) = native
    {
        copied = native.set_text(text).is_ok();
    }

    let remote = is_remote_session(environment);
    if copied && !remote {
        return Ok(());
    }

    if !copied {
        copied = match platform {
            "darwin" => run_shell_with_input("pbcopy", text).await,
            "win32" => run_shell_with_input("clip", text).await,
            _ => copy_on_linux(text, environment).await,
        };
    }

    if remote || !copied {
        let osc52_copied = emit_osc52(text);
        copied = copied || osc52_copied;
    }

    if copied {
        Ok(())
    } else {
        Err(ClipboardError::new("Failed to copy to clipboard"))
    }
}

async fn read_wayland_clipboard_text() -> Option<String> {
    let bytes = run_command_with_output(
        "wl-paste",
        &["--no-newline", "--type", "text"],
        READ_TIMEOUT,
        READ_MAX_BUFFER_BYTES,
    )
    .await?;
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

async fn copy_on_linux(text: &str, environment: &ClipboardEnvironment) -> bool {
    if environment
        .variable("TERMUX_VERSION")
        .is_some_and(|value| !value.is_empty())
        && run_shell_with_input("termux-clipboard-set", text).await
    {
        return true;
    }

    let has_wayland_display = environment
        .variable("WAYLAND_DISPLAY")
        .is_some_and(|value| !value.is_empty());
    let has_x11_display = environment.variable("DISPLAY").is_some_and(|value| !value.is_empty());

    if is_wayland_session(environment) && has_wayland_display {
        // `which` catches a missing wl-copy: spawn errors would be async.
        if run_shell("which wl-copy").await {
            if run_program_with_input("wl-copy", text).await {
                return true;
            }
            return has_x11_display && copy_to_x11_clipboard(text).await;
        }
        return has_x11_display && copy_to_x11_clipboard(text).await;
    }

    has_x11_display && copy_to_x11_clipboard(text).await
}

async fn copy_to_x11_clipboard(text: &str) -> bool {
    if run_shell_with_input("xclip -selection clipboard", text).await {
        return true;
    }
    run_shell_with_input("xsel --clipboard --input", text).await
}

/// `execSync(command)` with `input` and the copy timeout.
async fn run_shell_with_input(command: &str, text: &str) -> bool {
    let mut child = match shell_command(command)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
    {
        Ok(child) => child,
        Err(_) => return false,
    };

    write_stdin(&mut child, text).await;
    wait_for_success(child).await
}

/// `execSync(command)` with ignored stdio.
async fn run_shell(command: &str) -> bool {
    let child = match shell_command(command)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
    {
        Ok(child) => child,
        Err(_) => return false,
    };

    wait_for_success(child).await
}

/// `spawn(program)` with the text written to stdin.
async fn run_program_with_input(program: &str, text: &str) -> bool {
    let mut child = match Command::new(program)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
    {
        Ok(child) => child,
        Err(_) => return false,
    };

    write_stdin(&mut child, text).await;
    wait_for_success(child).await
}

async fn write_stdin(child: &mut tokio::process::Child, text: &str) {
    if let Some(mut stdin) = child.stdin.take() {
        // EPIPE is expected when the tool exits early.
        let _ = stdin.write_all(text.as_bytes()).await;
        let _ = stdin.shutdown().await;
    }
}

async fn wait_for_success(mut child: tokio::process::Child) -> bool {
    match tokio::time::timeout(COPY_TIMEOUT, child.wait()).await {
        Ok(Ok(status)) => status.success(),
        _ => false,
    }
}

#[cfg(unix)]
fn shell_command(command: &str) -> Command {
    let mut shell = Command::new("/bin/sh");
    shell.arg("-c").arg(command);
    shell
}

#[cfg(windows)]
fn shell_command(command: &str) -> Command {
    let mut shell = Command::new("cmd.exe");
    shell.arg("/C").arg(command);
    shell
}

/// Runs a command and returns its stdout when it exits successfully.
async fn run_command_with_output(
    program: &str,
    arguments: &[&str],
    timeout: Duration,
    max_buffer_bytes: usize,
) -> Option<Vec<u8>> {
    use tokio::io::AsyncReadExt;

    let mut child = Command::new(program)
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .ok()?;

    let mut stdout = child.stdout.take()?;
    let mut buffer = Vec::new();
    let collect = async {
        let mut limited = (&mut stdout).take(max_buffer_bytes as u64 + 1);
        limited.read_to_end(&mut buffer).await.ok()?;
        child.wait().await.ok()
    };

    let status = tokio::time::timeout(timeout, collect).await.ok()??;
    if !status.success() || buffer.len() > max_buffer_bytes {
        return None;
    }
    Some(buffer)
}
