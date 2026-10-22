//! Choose built-in tools by name, including a custom working directory.
mod error;
use pi_coding_agent::core::{
    sdk::{CreateAgentSessionOptions, create_agent_session},
    session_manager::SessionManager,
};
use pi_js_compat::js_json::JsString;
use std::io::Write;

/// Return the observed cwd and active names for embedders without extra stdout.
pub async fn run(
    options: [CreateAgentSessionOptions; 4],
    custom_cwd: &str,
    mut output: impl Write,
) -> Result<Vec<(JsString, Vec<String>)>, JsString> {
    let choices: &[(&[&str], &str)] = &[
        (&["read", "grep", "find", "ls"], "Read-only session created"),
        (&["read", "bash", "grep"], "Custom tools session created"),
        (&["read", "bash", "edit", "write"], "Custom cwd session created"),
        (
            &["read", "bash", "grep"],
            "Specific tools with custom cwd session created",
        ),
    ];
    let mut selections = Vec::new();
    for (index, (mut options, (names, message))) in options.into_iter().zip(choices).enumerate() {
        if index >= 2 {
            options.cwd = Some(custom_cwd.into());
        }
        options.tools = Some(names.iter().map(|name| (*name).into()).collect());
        options.session_manager =
            Some(SessionManager::in_memory(options.cwd.as_ref(), None).map_err(|e| e.message().clone())?);
        let session = create_agent_session(options).await?.session;
        writeln!(output, "{message}").map_err(|e| JsString::from(e.to_string()))?;
        selections.push((session.cwd().to_owned(), session.get_active_tool_names()));
        session.dispose();
    }
    Ok(selections)
}
#[tokio::main(flavor = "current_thread")]
pub async fn main() -> Result<(), error::ExampleError> {
    run(
        std::array::from_fn(|_| CreateAgentSessionOptions::default()),
        "/path/to/project",
        std::io::stdout(),
    )
    .await
    .map(|_| ())
    .map_err(error::ExampleError)
}
