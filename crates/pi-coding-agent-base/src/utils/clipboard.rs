//! Frozen `utils/clipboard.ts`: prefer platform clipboard tools, then OSC52.

use std::io::Write;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use pi_js_compat::base64::buffer_to_base64;
use pi_js_compat::js_format;
use pi_js_compat::js_json::{JsString, JsValue, encode_node_utf8};
use pi_js_compat::node_error::NativeNodeError;
use tokio::io::AsyncWriteExt;

use crate::utils::child_process::{
    ChildStdio, ChildStdioOptions, SpawnSyncOptions, node_error_code, spawn_process, spawn_sync,
};
use crate::utils::clipboard_image::{ClipboardEnvironment, is_wayland_session};
use crate::utils::clipboard_native::{ClipboardError, ClipboardModule};

const MAX_OSC52_ENCODED_LENGTH: usize = 100_000;
const COPY_TIMEOUT: Duration = Duration::from_millis(5000);

/// Whether the session runs over SSH or Mosh.
pub fn is_remote_session(environment: &ClipboardEnvironment) -> bool {
    ["SSH_CONNECTION", "SSH_CLIENT", "MOSH_CONNECTION"]
        .iter()
        .any(|name| environment.variable(name).is_some_and(|value| !value.is_empty()))
}

/// `emitOsc52`: the size cap returns false; an unhandled stdout error stays uncaught.
pub fn emit_osc52(text: &str) -> bool {
    let encoded = buffer_to_base64(text.as_bytes());
    if encoded.len() > MAX_OSC52_ENCODED_LENGTH {
        return false;
    }
    let mut stdout = std::io::stdout().lock();
    let result = write!(stdout, "\u{1b}]52;c;{encoded}\u{7}").and_then(|()| stdout.flush());
    drop(stdout);
    if let Err(error) = result {
        defer_uncaught(node_io_error(&error, "write".into()));
    }
    true
}

/// Copies text, rejecting only when no source mechanism succeeds.
/// Node UTF-8 encoding occurs at the native/child/Buffer boundary.
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
            "darwin" => run_shell_with_input("pbcopy", text),
            "win32" => run_shell_with_input("clip", text),
            _ => copy_on_linux(text, environment),
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

fn copy_on_linux(text: &str, environment: &ClipboardEnvironment) -> bool {
    if environment
        .variable("TERMUX_VERSION")
        .is_some_and(|value| !value.is_empty())
        && run_shell_with_input("termux-clipboard-set", text)
    {
        return true;
    }
    let has_wayland_display = environment
        .variable("WAYLAND_DISPLAY")
        .is_some_and(|value| !value.is_empty());
    let has_x11_display = environment.variable("DISPLAY").is_some_and(|value| !value.is_empty());
    if is_wayland_session(environment) && has_wayland_display {
        if run_shell("which wl-copy", None, None) && spawn_wl_copy(text) {
            return true;
        }
        return has_x11_display && copy_to_x11_clipboard(text);
    }
    has_x11_display && copy_to_x11_clipboard(text)
}

fn copy_to_x11_clipboard(text: &str) -> bool {
    run_shell_with_input("xclip -selection clipboard", text) || run_shell_with_input("xsel --clipboard --input", text)
}

fn run_shell_with_input(command: &str, text: &str) -> bool {
    run_shell(command, Some(text.as_bytes()), Some(COPY_TIMEOUT))
}

/// Source execSync catches failures; its timeout includes stdin delivery.
fn run_shell(command: &str, input: Option<&[u8]>, timeout: Option<Duration>) -> bool {
    let arguments: Vec<JsString> = vec!["-c".into(), command.into()];
    let result = spawn_sync(
        &"/bin/sh".into(),
        &arguments,
        SpawnSyncOptions {
            input,
            timeout,
            stdio: ChildStdioOptions {
                stdin: if input.is_some() {
                    ChildStdio::Pipe
                } else {
                    ChildStdio::Ignore
                },
                stdout: ChildStdio::Ignore,
                stderr: ChildStdio::Ignore,
            },
            ..Default::default()
        },
    );
    result.is_ok_and(|result| result.error.is_none() && result.status == Some(0))
}

/// Frozen :92-103 never waits for wl-copy's status or imposes a timeout.
fn spawn_wl_copy(text: &str) -> bool {
    let mut child = match spawn_process(
        "wl-copy",
        &[],
        ChildStdioOptions {
            stdin: ChildStdio::Pipe,
            stdout: ChildStdio::Ignore,
            stderr: ChildStdio::Ignore,
        },
    ) {
        Ok(child) => child,
        Err(error) if pi_js_compat::node_child_process::spawn_error_is_async(&error) => {
            let error = node_io_error(&error, "spawn wl-copy".into())
                .with_property("path", JsValue::String("wl-copy".into()))
                .with_property("spawnargs", JsValue::Array(Vec::new()));
            defer_uncaught(error);
            return true;
        }
        Err(_) => return false,
    };
    let mut stdin = child.stdin.take().expect("wl-copy stdin is explicitly piped");
    let bytes = text.as_bytes().to_vec();
    tokio::spawn(async move {
        // Frozen stdin.on("error", () => {}) deliberately ignores write errors.
        let _ = stdin.write_all(&bytes).await;
        let _ = stdin.shutdown().await;
        drop(stdin);
        // Drop does not kill the process: source unref retains selection ownership.
        drop(child);
    });
    true
}

fn node_io_error(error: &std::io::Error, syscall: JsString) -> NativeNodeError {
    let code = node_error_code(error);
    let mut native = NativeNodeError::new("Error", js_format!("{} {}", syscall, code));
    if let Some(errno) = error.raw_os_error() {
        native = native.with_property("errno", JsValue::Number(-f64::from(errno)));
    }
    native
        .with_property("code", JsValue::String(code.into()))
        .with_property("syscall", JsValue::String(syscall))
}

fn defer_uncaught(error: NativeNodeError) {
    let slot = Arc::new(Mutex::new(Some(Box::new(error))));
    crate::utils::uncaught::defer_native_error(slot, &tokio::runtime::Handle::current());
}
