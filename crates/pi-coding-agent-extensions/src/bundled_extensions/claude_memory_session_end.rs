//! Bundled `claude-memory-session-end` extension (`extensions/claude-memory-session-end/src/index.ts`).

use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;
use std::process::Stdio;

use pi_js_compat::js_format;
use pi_js_compat::js_json::{JsString, encode_node_utf8};
use pi_js_compat::process_env;

use crate::core::extensions::types::{BoxFuture, ExtensionApi, ExtensionContext, HandlerError, SessionShutdownEvent};
use crate::utils::child_process::node_spawn_error_message;
use crate::utils::iso_time::now_iso_string;

const CLAUDE_MEMORY: &str = "/home/osso/.cargo/bin/claude-memory";

pub fn claude_memory_session_end_extension(pi: &ExtensionApi) -> Result<(), HandlerError> {
    pi.on_session_shutdown(
        move |_event: SessionShutdownEvent,
              ctx: ExtensionContext|
              -> BoxFuture<'static, Result<Option<()>, HandlerError>> {
            Box::pin(async move {
                let session_file = ctx.session_manager()?.get_session_file();
                if let Some(session_file) = session_file {
                    launch_index(&session_file)?;
                }
                Ok(None)
            })
        },
    )
    .map_err(HandlerError::new)
}

/// One log line; the log file write is the UTF-8 boundary.
fn format_log_line(message: &JsString) -> String {
    format!("[{}] {}\n", now_iso_string(), encode_node_utf8(message))
}

fn append_log_line(log_path: &Path, message: &JsString) -> std::io::Result<()> {
    if let Some(parent) = log_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut file = OpenOptions::new().create(true).append(true).open(log_path)?;
    file.write_all(format_log_line(message).as_bytes())
}

fn report_launch_failure(log_path: &Path, session_path: &JsString, executable: &str, error: &std::io::Error) {
    let message = js_format!(
        "failed to launch index-file for session {}: {}",
        session_path,
        spawn_error_message(executable, error)
    );
    if let Err(log_error) = append_log_line(log_path, &message) {
        eprintln!(
            "{}; failed to write {}: {}",
            encode_node_utf8(&message),
            log_path.display(),
            error_message(&log_error)
        );
    }
}

fn launch_index(session_path: &JsString) -> Result<(), HandlerError> {
    let cache_home = process_env::get("XDG_CACHE_HOME")
        .unwrap_or_else(|| format!("{}/.cache", pi_js_compat::process_env::get("HOME").unwrap_or_default()));
    let log_path = Path::new(&cache_home).join("claude-memory").join("pi-index.log");
    launch_index_with(session_path, CLAUDE_MEMORY, &log_path)
}

fn launch_index_with(session_path: &JsString, executable: &str, log_path: &Path) -> Result<(), HandlerError> {
    if let Some(parent) = log_path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| HandlerError::new(error_message(&error)))?;
    }
    let mut log_file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)
        .map_err(|error| HandlerError::new(error_message(&error)))?;
    log_file
        .write_all(format_log_line(&js_format!("starting index-file for session {}", session_path)).as_bytes())
        .map_err(|error| HandlerError::new(error_message(&error)))?;

    let spawn = {
        use std::os::unix::process::CommandExt;
        std::process::Command::new(executable)
            .arg("index-file")
            .arg(encode_node_utf8(session_path))
            .stdin(Stdio::null())
            .stdout(Stdio::from(
                log_file
                    .try_clone()
                    .map_err(|error| HandlerError::new(error_message(&error)))?,
            ))
            .stderr(Stdio::from(log_file))
            .process_group(0)
            .spawn()
    };
    match spawn {
        Ok(child) => {
            drop(child);
            Ok(())
        }
        Err(error) => {
            report_launch_failure(log_path, session_path, executable, &error);
            Ok(())
        }
    }
}

fn spawn_error_message(executable: &str, error: &std::io::Error) -> JsString {
    node_spawn_error_message("spawn", &JsString::from(executable), error)
}

fn error_message(error: &std::io::Error) -> String {
    error.to_string()
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    use tempfile::TempDir;

    use super::*;

    fn write_script(directory: &Path, body: &str) -> String {
        let path = directory.join("fake-claude-memory");
        fs::write(&path, body).unwrap();
        let mut permissions = fs::metadata(&path).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&path, permissions).unwrap();
        path.to_string_lossy().into_owned()
    }

    #[test]
    fn launches_index_file_detached_and_logs_the_start() {
        let temp = TempDir::new().unwrap();
        let script = write_script(temp.path(), "#!/bin/sh\nsleep 5\n");
        let log_path = temp.path().join("claude-memory/pi-index.log");

        launch_index_with(&"/tmp/pi fixture/session.jsonl".into(), &script, &log_path).unwrap();

        let log = fs::read_to_string(&log_path).unwrap();
        assert!(log.contains("starting index-file for session /tmp/pi fixture/session.jsonl"));
        assert!(log.contains("] starting index-file"));
    }

    #[test]
    fn logs_spawn_failures_without_failing() {
        let temp = TempDir::new().unwrap();
        let log_path = temp.path().join("claude-memory/pi-index.log");

        launch_index_with(
            &"/tmp/failure.jsonl".into(),
            "/definitely/missing/claude-memory",
            &log_path,
        )
        .unwrap();

        let log = fs::read_to_string(&log_path).unwrap();
        assert!(log.contains("failed to launch index-file for session /tmp/failure.jsonl"));
        assert!(log.contains("spawn /definitely/missing/claude-memory ENOENT"));
    }
}
