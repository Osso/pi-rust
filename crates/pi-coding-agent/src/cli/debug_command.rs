//! `pi debug` command (`cli/debug-command.ts`).
//!
//! `core/debug-repl.ts` is a documented gap (`node:vm`), so the socket path helper
//! is reproduced here: `join(agentDir, "debug", `${pid}.sock`)`.

use std::io::{Read, Write};
use std::path::Path;

use pi_ai::request::BoxFuture;

use crate::config::get_user_state_root;
use crate::core::session_control_db::{get_control_db_path, read_session_health};
use crate::core::session_health::SessionCheckStatus;

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
    fn agent_dir(&self) -> String {
        crate::config::get_agent_dir().display().to_string()
    }

    /// `getControlDbPath()`.
    fn control_db_path(&self) -> String {
        get_control_db_path(&get_user_state_root())
    }

    fn stdout(&self, text: &str) {
        let mut stdout = std::io::stdout().lock();
        let _ = stdout.write_all(text.as_bytes());
        let _ = stdout.flush();
    }

    fn stderr(&self, text: &str) {
        let mut stderr = std::io::stderr().lock();
        let _ = stderr.write_all(text.as_bytes());
        let _ = stderr.flush();
    }

    fn attach<'a>(&'a self, socket_path: &'a str, session_id: &'a str) -> BoxFuture<'a, Result<(), String>> {
        Box::pin(async move { attach_debug_repl(socket_path, session_id) })
    }
}

fn get_debug_socket_path(agent_dir: &str, pid: u32) -> String {
    Path::new(agent_dir)
        .join("debug")
        .join(format!("{pid}.sock"))
        .display()
        .to_string()
}

/// `handleDebugCommand`.
pub async fn handle_debug_command(args: &[String], dependencies: &dyn DebugCommandDependencies) -> DebugCommandOutcome {
    if args.first().map(String::as_str) != Some("debug") {
        return DebugCommandOutcome {
            handled: false,
            exit_code: None,
        };
    }

    if args.len() == 2 && matches!(args[1].as_str(), "--help" | "-h") {
        dependencies.stdout(USAGE);
        return DebugCommandOutcome::handled(Some(0));
    }
    if args.len() != 3 || args[1] != "attach" {
        dependencies.stderr(USAGE);
        return DebugCommandOutcome::handled(Some(1));
    }

    let session_id = args[2].as_str();
    let health = read_session_health(&dependencies.control_db_path(), session_id)
        .ok()
        .flatten();
    let pid = health
        .filter(|health| health.check_status == SessionCheckStatus::Ok)
        .and_then(|health| health.pid);
    let Some(pid) = pid else {
        dependencies.stderr(&format!("Session {session_id} is not running.\n"));
        return DebugCommandOutcome::handled(Some(1));
    };

    let socket_path = get_debug_socket_path(&dependencies.agent_dir(), pid);
    if let Err(error) = dependencies.attach(&socket_path, session_id).await {
        dependencies.stderr(&format!(
            "Could not attach to debug REPL for session {session_id}: {error}\n"
        ));
        return DebugCommandOutcome::handled(Some(1));
    }
    DebugCommandOutcome::handled(None)
}

/// `attachDebugRepl`: handshake with the owning process, then pipe stdio.
#[cfg(unix)]
pub fn attach_debug_repl(socket_path: &str, session_id: &str) -> Result<(), String> {
    use std::os::unix::net::UnixStream;

    let mut socket = UnixStream::connect(socket_path).map_err(|error| error.to_string())?;
    let handshake = serde_json::json!({"pid": std::process::id(), "sessionId": session_id});
    socket
        .write_all(format!("{handshake}\n").as_bytes())
        .map_err(|error| error.to_string())?;

    let mut input_socket = socket.try_clone().map_err(|error| error.to_string())?;
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
                return Err(error.to_string());
            }
        }
    }
    let _ = stdout.flush();
    let _ = input.join();
    Ok(())
}

#[cfg(not(unix))]
pub fn attach_debug_repl(_socket_path: &str, _session_id: &str) -> Result<(), String> {
    Err("Debug REPL attach requires a Unix socket".to_owned())
}
