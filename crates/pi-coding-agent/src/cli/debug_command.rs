//! `pi debug` command (`cli/debug-command.ts`).
//!
//! `core/debug-repl.ts` is a documented gap (`node:vm`), so the socket path helper
//! is reproduced here: `join(agentDir, "debug", `${pid}.sock`)`.

use pi_ai::request::CallbackError;
use std::io::{Read, Write};

use pi_ai::request::BoxFuture;
use pi_js_compat::js_format;
use pi_js_compat::js_json::{JsString, JsValue, encode_node_utf8, stringify_lossless};
use pi_js_compat::node_fs::node_unix_connect_error;

use crate::config::get_user_state_root;
use crate::core::session_control_db::{get_control_db_path, read_session_health};
use crate::core::session_health::SessionCheckStatus;
use crate::utils::paths::node_join;

const USAGE: &str = "Usage: pi debug attach <session-id>\n";

/// Result of handling one CLI invocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DebugCommandOutcome {
    pub handled: bool,
    pub exit_code: Option<i32>,
}

impl DebugCommandOutcome {
    const fn handled(exit_code: Option<i32>) -> Self {
        Self {
            handled: true,
            exit_code,
        }
    }
}

/// Injectable effects used by [`handle_debug_command`].
pub trait DebugCommandDependencies {
    fn agent_dir(&self) -> Result<JsString, CallbackError> {
        crate::config::get_agent_dir().map_err(CallbackError::from)
    }

    /// `getControlDbPath()`.
    fn control_db_path(&self) -> Result<JsString, CallbackError> {
        Ok(get_control_db_path(
            &get_user_state_root().map_err(CallbackError::from)?,
        ))
    }

    fn stdout(&self, text: &JsString) {
        let mut stdout = std::io::stdout().lock();
        let _ = stdout.write_all(encode_node_utf8(text).as_bytes());
        let _ = stdout.flush();
    }

    fn stderr(&self, text: &JsString) {
        let mut stderr = std::io::stderr().lock();
        let _ = stderr.write_all(encode_node_utf8(text).as_bytes());
        let _ = stderr.flush();
    }

    fn attach<'a>(
        &'a self,
        socket_path: &'a JsString,
        session_id: &'a JsString,
    ) -> BoxFuture<'a, Result<(), JsString>> {
        Box::pin(async move { attach_debug_repl(socket_path, session_id) })
    }
}

/// `getDebugSocketPath`: `join(agentDir, "debug", `${pid}.sock`)`.
fn get_debug_socket_path(agent_dir: &JsString, pid: u32) -> JsString {
    node_join(node_join(agent_dir, "debug"), js_format!("{}.sock", pid))
}

/// `handleDebugCommand`. A control-DB read failure throws in TS and propagates here.
pub async fn handle_debug_command(
    args: &[String],
    dependencies: &dyn DebugCommandDependencies,
) -> Result<DebugCommandOutcome, CallbackError> {
    if args.first().map(String::as_str) != Some("debug") {
        return Ok(DebugCommandOutcome {
            handled: false,
            exit_code: None,
        });
    }

    if args.len() == 2 && matches!(args[1].as_str(), "--help" | "-h") {
        dependencies.stdout(&USAGE.into());
        return Ok(DebugCommandOutcome::handled(Some(0)));
    }
    if args.len() != 3 || args[1] != "attach" {
        dependencies.stderr(&USAGE.into());
        return Ok(DebugCommandOutcome::handled(Some(1)));
    }

    let session_id = JsString::from(args[2].as_str());
    let health = read_session_health(dependencies.control_db_path()?, &session_id).map_err(CallbackError::from)?;
    let pid = health
        .filter(|health| health.check_status == SessionCheckStatus::Ok)
        .and_then(|health| health.pid);
    let Some(pid) = pid else {
        dependencies.stderr(&js_format!("Session {} is not running.\n", session_id));
        return Ok(DebugCommandOutcome::handled(Some(1)));
    };

    let socket_path = get_debug_socket_path(&dependencies.agent_dir()?, pid);
    if let Err(error) = dependencies.attach(&socket_path, &session_id).await {
        dependencies.stderr(&js_format!(
            "Could not attach to debug REPL for session {}: {}\n",
            session_id,
            error
        ));
        return Ok(DebugCommandOutcome::handled(Some(1)));
    }
    Ok(DebugCommandOutcome::handled(None))
}

/// `attachDebugRepl`: handshake with the owning process, then pipe stdio.
#[cfg(unix)]
pub fn attach_debug_repl(socket_path: &JsString, session_id: &JsString) -> Result<(), JsString> {
    use std::os::unix::net::UnixStream;

    let mut socket = UnixStream::connect(encode_node_utf8(socket_path))
        .map_err(|error| node_unix_connect_error(&error, socket_path))?;
    // Lossless JSON escapes unpaired units, so the handshake text is well-formed.
    let handshake = stringify_lossless(&JsValue::Object(
        [
            ("pid".into(), JsValue::Number(f64::from(std::process::id()))),
            ("sessionId".into(), JsValue::String(session_id.clone())),
        ]
        .into_iter()
        .collect(),
    ));
    socket
        .write_all(format!("{handshake}\n").as_bytes())
        .map_err(|error| JsString::from(error.to_string()))?;

    let mut input_socket = socket.try_clone().map_err(|error| JsString::from(error.to_string()))?;
    let input = std::thread::spawn(move || {
        let mut stdin = std::io::stdin().lock();
        let _ = std::io::copy(&mut stdin, &mut input_socket);
        let _ = input_socket.shutdown(std::net::Shutdown::Write);
    });

    let mut stdout = std::io::stdout().lock();
    let mut buffer = [0_u8; 8192];
    loop {
        match socket.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => {
                let _ = stdout.write_all(&buffer[..read]);
                let _ = stdout.flush();
            }
            Err(error) => {
                let _ = input.join();
                return Err(JsString::from(error.to_string()));
            }
        }
    }
    let _ = stdout.flush();
    let _ = input.join();
    Ok(())
}

#[cfg(not(unix))]
pub fn attach_debug_repl(_socket_path: &JsString, _session_id: &JsString) -> Result<(), JsString> {
    Err(JsString::from("Debug REPL attach requires a Unix socket"))
}
