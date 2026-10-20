//! Disk settings, session-only overrides, durability and surfaced I/O errors.
mod error;
mod resources;
use pi_agent_core::ThinkingLevel;
use pi_coding_agent::core::{
    sdk::{CreateAgentSessionOptions, create_agent_session},
    session_manager::SessionManager,
    settings_manager::SettingsManager,
};
use serde_json::{Value, json};
use std::io::Write;
use std::path::Path;
use std::sync::{Arc, Mutex};

pub async fn run(
    options: [CreateAgentSessionOptions; 2],
    mut output: impl Write,
    mut warnings: impl Write,
) -> Result<Value, String> {
    let [mut custom, mut test] = options;
    let paths = resources::options(&custom)?;
    let from_disk = SettingsManager::create(&paths.cwd, Some(Path::new(&paths.agent_dir)), Default::default())
        .map_err(|e| e.to_string())?;
    writeln!(
        output,
        "Current settings: {}",
        serde_json::to_string_pretty(&from_disk.get_global_settings()).map_err(|e| e.to_string())?
    )
    .map_err(|e| e.to_string())?;
    let mut manager = SettingsManager::create(&paths.cwd, Some(Path::new(&paths.agent_dir)), Default::default())
        .map_err(|e| e.to_string())?;
    manager.apply_overrides(
        json!({"compaction":{"enabled":false},"retry":{"enabled":true,"maxRetries":5,"baseDelayMs":1000}})
            .as_object()
            .expect("settings object"),
    );
    let manager = Arc::new(Mutex::new(manager));
    custom.settings_manager = Some(Arc::clone(&manager));
    custom.session_manager = Some(SessionManager::in_memory(custom.cwd.as_deref(), None).map_err(|e| e.to_string())?);
    let session = create_agent_session(custom).await?.session;
    writeln!(output, "Session created with custom settings").map_err(|e| e.to_string())?;
    session.dispose();
    let snapshot = {
        let mut manager = manager.lock().expect("settings mutex poisoned");
        let snapshot = serde_json::to_value(manager.get_merged_settings()).map_err(|e| e.to_string())?;
        manager.set_default_thinking_level(ThinkingLevel::Low);
        manager.flush();
        for error in manager.drain_errors() {
            writeln!(
                warnings,
                "Warning ({} settings): {}",
                error.scope.as_str(),
                error.message
            )
            .map_err(|e| e.to_string())?;
        }
        snapshot
    };
    test.settings_manager = Some(Arc::new(Mutex::new(SettingsManager::in_memory(
        json!({"compaction":{"enabled":false},"retry":{"enabled":false}})
            .as_object()
            .expect("settings object")
            .clone(),
        Default::default(),
    ))));
    test.session_manager = Some(SessionManager::in_memory(test.cwd.as_deref(), None).map_err(|e| e.to_string())?);
    let session = create_agent_session(test).await?.session;
    writeln!(output, "Test session created with in-memory settings").map_err(|e| e.to_string())?;
    session.dispose();
    Ok(snapshot)
}
#[tokio::main(flavor = "current_thread")]
pub async fn main() -> Result<(), error::ExampleError> {
    run(
        std::array::from_fn(|_| CreateAgentSessionOptions::default()),
        std::io::stdout(),
        std::io::stderr(),
    )
    .await
    .map(|_| ())
    .map_err(error::ExampleError)
}
