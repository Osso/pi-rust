//! `pi control` command (`cli/control-command.ts`).

use pi_ai::request::CallbackError;
use std::io::Write;

use pi_js_compat::js_format;
use pi_js_compat::js_json::{JsString, encode_node_utf8};
use rustix::process::{Pid, Signal, kill_process};

use crate::config::get_user_state_root;
use crate::core::session_control_db::{
    enqueue_incoming_message, get_control_db_path, read_last_message, read_session_health,
};
use crate::core::session_health::SessionCheckStatus;

/// Result of handling one CLI invocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ControlCommandOutcome {
    pub handled: bool,
    pub exit_code: Option<i32>,
}

impl ControlCommandOutcome {
    const fn handled() -> Self {
        Self {
            handled: true,
            exit_code: None,
        }
    }

    const fn failed() -> Self {
        Self {
            handled: true,
            exit_code: Some(1),
        }
    }
}

/// Injectable effects used by [`handle_control_command`].
pub trait ControlCommandDependencies {
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

    /// `getControlDbPath()`.
    fn control_db_path(&self) -> Result<JsString, CallbackError> {
        Ok(get_control_db_path(&get_user_state_root().map_err(CallbackError::from)?).into())
    }

    /// `process.kill(pid, "SIGHUP")`.
    fn signal_process(&self, pid: u32) -> Result<(), String> {
        let pid = i32::try_from(pid).map_err(|error| error.to_string())?;
        let pid = Pid::from_raw(pid).ok_or_else(|| "invalid pid".to_owned())?;
        kill_process(pid, Signal::HUP).map_err(|error| {
            let code = pi_js_compat::node_fs::node_error_code(&std::io::Error::from(error));
            format!("kill {code}")
        })
    }
}

/// `handleControlCommand`.
pub fn handle_control_command(
    args: &[String],
    dependencies: &dyn ControlCommandDependencies,
) -> Result<ControlCommandOutcome, CallbackError> {
    if args.first().map(String::as_str) != Some("control") {
        return Ok(ControlCommandOutcome {
            handled: false,
            exit_code: None,
        });
    }

    match args.get(1).map(String::as_str) {
        Some("send") => send(args, dependencies),
        Some("restart") => restart(args, dependencies),
        Some("last") => last(dependencies),
        Some("path") => {
            dependencies.stdout(&encode_node_utf8(&js_format!("{}\n", dependencies.control_db_path()?)));
            Ok(ControlCommandOutcome::handled())
        }
        other => {
            let help = matches!(other, Some("--help" | "-h"));
            print_control_help(dependencies, help);
            Ok(ControlCommandOutcome {
                handled: true,
                exit_code: Some(if help { 0 } else { 1 }),
            })
        }
    }
}

fn send(
    args: &[String],
    dependencies: &dyn ControlCommandDependencies,
) -> Result<ControlCommandOutcome, CallbackError> {
    let send_args = &args[2..];
    if send_args.iter().any(|arg| matches!(arg.as_str(), "--help" | "-h")) {
        print_control_help(dependencies, true);
        return Ok(ControlCommandOutcome {
            handled: true,
            exit_code: Some(0),
        });
    }
    let Some(parsed) = parse_send_command(send_args) else {
        print_control_help(dependencies, false);
        return Ok(ControlCommandOutcome::failed());
    };

    let id =
        enqueue_incoming_message(&dependencies.control_db_path()?, &parsed.message).map_err(CallbackError::from)?;
    dependencies.stdout(&format!("queued {id}\n"));
    if let Some(pid) = parsed.pid {
        dependencies.signal_process(pid).map_err(JsString::from)?;
        dependencies.stdout(&format!("signaled {pid}\n"));
    }
    Ok(ControlCommandOutcome::handled())
}

fn restart(
    args: &[String],
    dependencies: &dyn ControlCommandDependencies,
) -> Result<ControlCommandOutcome, CallbackError> {
    let Some(session_id) = parse_restart_command(&args[2..]) else {
        print_control_help(dependencies, false);
        return Ok(ControlCommandOutcome::failed());
    };

    let health = read_session_health(dependencies.control_db_path()?, &JsString::from(session_id.as_str()))
        .map_err(CallbackError::from)?;
    let pid = health
        .filter(|health| health.check_status == SessionCheckStatus::Ok)
        .and_then(|health| health.pid);
    let Some(pid) = pid else {
        dependencies.stderr(&format!("Session {session_id} is not running.\n"));
        return Ok(ControlCommandOutcome::failed());
    };

    if let Err(error) = dependencies.signal_process(pid) {
        dependencies.stderr(&format!("Could not restart session {session_id}: {error}\n"));
        return Ok(ControlCommandOutcome::failed());
    }
    dependencies.stdout(&format!("signaled session {session_id} (pid {pid})\n"));
    Ok(ControlCommandOutcome::handled())
}

fn last(dependencies: &dyn ControlCommandDependencies) -> Result<ControlCommandOutcome, CallbackError> {
    let last_message = read_last_message(dependencies.control_db_path()?).map_err(CallbackError::from)?;
    if let Some(last_message) = last_message {
        dependencies.stdout(&format!("{}\n", last_message.content));
    }
    Ok(ControlCommandOutcome::handled())
}

struct ParsedSendCommand {
    message: String,
    pid: Option<u32>,
}

fn parse_send_command(args: &[String]) -> Option<ParsedSendCommand> {
    let mut pid = None;
    let mut message_parts: Vec<&str> = Vec::new();
    let mut index = 0;

    while let Some(arg) = args.get(index) {
        if arg == "--pid" {
            index += 1;
            let parsed_pid = args
                .get(index)
                .map(|raw| pi_js_compat::number::parse_number(raw))
                .unwrap_or(f64::NAN);
            if parsed_pid <= 0.0 || parsed_pid.fract() != 0.0 {
                return None;
            }
            pid = Some(parsed_pid as u32);
            index += 1;
            continue;
        }
        message_parts.push(arg);
        index += 1;
    }

    let message = pi_js_compat::string::js_trim(&message_parts.join(" ")).to_owned();
    if message.is_empty() {
        return None;
    }
    Some(ParsedSendCommand { message, pid })
}

fn parse_restart_command(args: &[String]) -> Option<String> {
    if args.len() != 2 || args[0] != "--session-id" {
        return None;
    }
    let session_id = pi_js_compat::string::js_trim(&args[1]);
    (!session_id.is_empty()).then(|| session_id.to_owned())
}

fn print_control_help(dependencies: &dyn ControlCommandDependencies, to_stdout: bool) {
    let help = "Usage:\n  pi control send [--pid <pid>] <message>\n  pi control restart --session-id <session-id>\n  pi control last\n  pi control path\n";
    if to_stdout {
        dependencies.stdout(help);
    } else {
        dependencies.stderr(help);
    }
}
