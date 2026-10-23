//! Disk settings, session-only overrides, durability and surfaced I/O errors.
mod error;
mod resources;
use pi_agent_core::ThinkingLevel;
use pi_coding_agent::core::{
    sdk::{CreateAgentSessionOptions, create_agent_session},
    session_manager::SessionManager,
    settings_manager::SettingsManager,
};
use pi_js_compat::js_json;
use pi_js_compat::js_json::JsString;
use pi_js_compat::js_json::JsValue;
use pi_js_compat::js_json::encode_node_utf8;
use std::io::Write;
use std::sync::{Arc, Mutex};

pub async fn run(
    options: [CreateAgentSessionOptions; 2],
    mut output: impl Write,
    mut warnings: impl Write,
) -> Result<JsValue, pi_ai::request::CallbackError> {
    let [mut custom, mut test] = options;
    let paths = resources::options(&custom)?;
    let from_disk = SettingsManager::create(&paths.cwd, Some(&paths.agent_dir), Default::default())
        .map_err(pi_ai::request::CallbackError::from)?;
    writeln!(
        output,
        "Current settings: {}",
        js_json::stringify_lossless_pretty(&JsValue::Object(from_disk.get_global_settings()))
    )
    .map_err(|e| JsString::from(e.to_string()))?;
    let mut manager = SettingsManager::create(&paths.cwd, Some(&paths.agent_dir), Default::default())
        .map_err(pi_ai::request::CallbackError::from)?;
    manager.apply_overrides(
        js_json!({"compaction":{"enabled":false},"retry":{"enabled":true,"maxRetries":5,"baseDelayMs":1000}})
            .as_object()
            .expect("settings object"),
    );
    let manager = Arc::new(Mutex::new(manager));
    custom.settings_manager = Some(Arc::clone(&manager));
    custom.session_manager =
        Some(SessionManager::in_memory(custom.cwd.as_ref(), None).map_err(pi_ai::request::CallbackError::from)?);
    let session = create_agent_session(custom)
        .await
        .map_err(pi_ai::request::CallbackError::from)?
        .session;
    writeln!(output, "Session created with custom settings").map_err(|e| JsString::from(e.to_string()))?;
    session.dispose();
    let snapshot = {
        let mut manager = manager.lock().expect("settings mutex poisoned");
        let snapshot = JsValue::Object(manager.get_merged_settings());
        manager.set_default_thinking_level(ThinkingLevel::Low);
        manager.flush();
        for error in manager.drain_errors() {
            writeln!(
                warnings,
                "Warning ({} settings): {}",
                error.scope.as_str(),
                encode_node_utf8(&error.message)
            )
            .map_err(|e| JsString::from(e.to_string()))?;
        }
        snapshot
    };
    test.settings_manager = Some(Arc::new(Mutex::new(SettingsManager::in_memory(
        js_json!({"compaction":{"enabled":false},"retry":{"enabled":false}})
            .as_object()
            .expect("settings object")
            .clone(),
        Default::default(),
    ))));
    test.session_manager =
        Some(SessionManager::in_memory(test.cwd.as_ref(), None).map_err(pi_ai::request::CallbackError::from)?);
    let session = create_agent_session(test)
        .await
        .map_err(pi_ai::request::CallbackError::from)?
        .session;
    writeln!(output, "Test session created with in-memory settings").map_err(|e| JsString::from(e.to_string()))?;
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
    .map_err(error::ExampleError::new)
}
