//! In-memory, new, continue, list and open session persistence.
mod error;
mod resources;
use pi_coding_agent::core::{
    sdk::{CreateAgentSessionOptions, create_agent_session},
    session_manager::{SessionInfo, SessionManager, get_default_session_dir},
};
use pi_js_compat::js_json::JsString;
use std::io::Write;

pub async fn run(
    options: [CreateAgentSessionOptions; 4],
    session_dir: Option<&str>,
    mut output: impl Write,
) -> Result<Vec<SessionInfo>, String> {
    let [mut memory, mut new, mut continued, mut opened] = options;
    let paths = resources::options(&memory)?;
    let cwd = paths.cwd;
    let dir = match session_dir {
        Some(dir) => dir.to_owned(),
        None => get_default_session_dir(&cwd, Some(&paths.agent_dir)).map_err(|e| e.to_string())?,
    };
    let control_db = memory.control_db_path.clone();
    memory.session_manager = Some(SessionManager::in_memory(Some(&cwd), None).map_err(|e| e.to_string())?);
    let session = create_agent_session(memory).await?.session;
    writeln!(
        output,
        "In-memory session: {}",
        session.session_file().as_ref().unwrap_or(&JsString::from("(none)"))
    )
    .map_err(|e| e.to_string())?;
    session.dispose();
    new.cwd = Some(cwd.clone());
    new.session_manager = Some(SessionManager::create(&cwd, Some(&dir), None).map_err(|e| e.to_string())?);
    let session = create_agent_session(new).await?.session;
    writeln!(
        output,
        "New session file: {}",
        session.session_file().as_ref().unwrap_or(&JsString::from("undefined"))
    )
    .map_err(|e| e.to_string())?;
    session.dispose();
    continued.cwd = Some(cwd.clone());
    continued.session_manager = Some(SessionManager::continue_recent(&cwd, Some(&dir)).map_err(|e| e.to_string())?);
    let result = create_agent_session(continued).await?;
    if let Some(message) = result.model_fallback_message {
        writeln!(output, "Note: {message}").map_err(|e| e.to_string())?;
    }
    writeln!(
        output,
        "Continued session: {}",
        result
            .session
            .session_file()
            .as_ref()
            .unwrap_or(&JsString::from("undefined"))
    )
    .map_err(|e| e.to_string())?;
    result.session.dispose();
    let sessions = SessionManager::list(&cwd, Some(&dir), None, control_db.as_ref()).map_err(|e| e.to_string())?;
    writeln!(output, "\nFound {} sessions:", sessions.len()).map_err(|e| e.to_string())?;
    for info in sessions.iter().take(3) {
        writeln!(
            output,
            "  {}... - \"{}...\"",
            info.id.chars().take(8).collect::<String>(),
            String::from_utf16_lossy(&info.first_message.encode_utf16().take(30).collect::<Vec<_>>())
        )
        .map_err(|e| e.to_string())?;
    }
    if let Some(info) = sessions.first() {
        opened.cwd = Some(cwd);
        opened.session_manager = Some(SessionManager::open(&info.path, Some(&dir), None).map_err(|e| e.to_string())?);
        let session = create_agent_session(opened).await?.session;
        writeln!(output, "\nOpened: {}", session.session_id()).map_err(|e| e.to_string())?;
        session.dispose();
    }
    Ok(sessions)
}
#[tokio::main(flavor = "current_thread")]
pub async fn main() -> Result<(), error::ExampleError> {
    run(
        std::array::from_fn(|_| CreateAgentSessionOptions::default()),
        None,
        std::io::stdout(),
    )
    .await
    .map(|_| ())
    .map_err(error::ExampleError)
}
