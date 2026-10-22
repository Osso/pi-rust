//! In-memory, new, continue, list and open session persistence.
mod error;
mod resources;
use pi_coding_agent::core::{
    sdk::{CreateAgentSessionOptions, create_agent_session},
    session_manager::{SessionInfo, SessionManager, get_default_session_dir},
};
use pi_js_compat::js_json::{JsString, encode_node_utf8};
use std::io::Write;

pub async fn run(
    options: [CreateAgentSessionOptions; 4],
    session_dir: Option<&JsString>,
    mut output: impl Write,
) -> Result<Vec<SessionInfo>, JsString> {
    let [mut memory, mut new, mut continued, mut opened] = options;
    let paths = resources::options(&memory)?;
    let cwd = paths.cwd;
    let dir = match session_dir {
        Some(dir) => dir.clone(),
        None => get_default_session_dir(&cwd, Some(&paths.agent_dir)).map_err(|e| e.message().clone())?,
    };
    let control_db = memory.control_db_path.clone();
    memory.session_manager = Some(SessionManager::in_memory(Some(&cwd), None).map_err(|e| e.message().clone())?);
    let session = create_agent_session(memory).await?.session;
    writeln!(
        output,
        "In-memory session: {}",
        display_session_file(session.session_file().as_ref(), "(none)")
    )
    .map_err(|e| JsString::from(e.to_string()))?;
    session.dispose();
    new.cwd = Some(cwd.clone());
    new.session_manager = Some(SessionManager::create(&cwd, Some(&dir), None).map_err(|e| e.message().clone())?);
    let session = create_agent_session(new).await?.session;
    writeln!(
        output,
        "New session file: {}",
        display_session_file(session.session_file().as_ref(), "undefined")
    )
    .map_err(|e| JsString::from(e.to_string()))?;
    session.dispose();
    continued.cwd = Some(cwd.clone());
    continued.session_manager =
        Some(SessionManager::continue_recent(&cwd, Some(&dir)).map_err(|e| e.message().clone())?);
    let result = create_agent_session(continued).await?;
    if let Some(message) = result.model_fallback_message {
        writeln!(output, "Note: {}", encode_node_utf8(&message)).map_err(|e| JsString::from(e.to_string()))?;
    }
    writeln!(
        output,
        "Continued session: {}",
        display_session_file(result.session.session_file().as_ref(), "undefined")
    )
    .map_err(|e| JsString::from(e.to_string()))?;
    result.session.dispose();
    let sessions =
        SessionManager::list(&cwd, Some(&dir), None, control_db.as_ref()).map_err(|e| e.message().clone())?;
    writeln!(output, "\nFound {} sessions:", sessions.len()).map_err(|e| JsString::from(e.to_string()))?;
    for info in sessions.iter().take(3) {
        writeln!(
            output,
            "  {}... - \"{}...\"",
            encode_node_utf8(&info.id.slice(0, 8)),
            encode_node_utf8(&info.first_message.slice(0, 30))
        )
        .map_err(|e| JsString::from(e.to_string()))?;
    }
    if let Some(info) = sessions.first() {
        opened.cwd = Some(cwd);
        opened.session_manager =
            Some(SessionManager::open(&info.path, Some(&dir), None).map_err(|e| e.message().clone())?);
        let session = create_agent_session(opened).await?.session;
        writeln!(output, "\nOpened: {}", encode_node_utf8(&session.session_id()))
            .map_err(|e| JsString::from(e.to_string()))?;
        session.dispose();
    }
    Ok(sessions)
}

fn display_session_file(file: Option<&JsString>, missing: &str) -> String {
    file.map_or_else(|| missing.to_owned(), encode_node_utf8)
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
