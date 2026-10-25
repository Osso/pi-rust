//! Early read-only terminal-export startup path from frozen `main.ts`.

use pi_ai::request::CallbackError;
use pi_js_compat::js_json::{JsString, encode_node_utf8};
use std::collections::BTreeMap;
use std::io::Write;

use crate::cli::args::{Args, DiagnosticType};
use crate::core::settings_manager::{
    InMemorySettingsStorage, SettingsManager, SettingsManagerCreateOptions, SettingsScope,
};
use crate::main_runner::{ResolvedSession, resolve_session_path};
use crate::modes::interactive::theme::{detect_terminal_background_from_env, init_theme, resolve_theme_setting};
use crate::utils::paths::{node_join, process_cwd};

fn read_terminal_export_settings() -> Result<SettingsManager, CallbackError> {
    let path = node_join(crate::config::get_agent_dir()?, "settings.json");
    let encoded = encode_node_utf8(&path);
    let content = if std::path::Path::new(&encoded).exists() {
        let bytes = std::fs::read(&encoded)
            .map_err(|error| CallbackError::from(pi_js_compat::node_fs::node_read_error(&error, &path)))?;
        Some(String::from_utf8_lossy(&bytes).into_owned())
    } else {
        None
    };
    let mut storage = InMemorySettingsStorage::new();
    storage.set(SettingsScope::Global, content.clone());
    storage.set(SettingsScope::Project, content);
    let mut settings = SettingsManager::from_storage(
        Box::new(storage),
        SettingsManagerCreateOptions {
            project_trusted: Some(false),
        },
    );
    if let Some(error) = settings.drain_errors().into_iter().next() {
        return Err(CallbackError::from(error.message));
    }
    Ok(settings)
}

pub(crate) fn export_terminal_history(parsed: Args) -> Result<i32, CallbackError> {
    for diagnostic in &parsed.diagnostics {
        let prefix = if diagnostic.kind == DiagnosticType::Error {
            "Error"
        } else {
            "Warning"
        };
        writeln!(std::io::stderr(), "{prefix}: {}", diagnostic.message)
            .map_err(|error| CallbackError::from(error.to_string()))?;
    }
    if parsed
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.kind == DiagnosticType::Error)
    {
        return Ok(1);
    }
    let (Some(session), Some(columns)) = (parsed.export_terminal, parsed.export_columns) else {
        return Err(CallbackError::from(
            "Terminal export requires a session and explicit columns",
        ));
    };
    if crate::core::external_session_importer::is_external_session_alias(&session.as_str().into()) {
        return Err(CallbackError::from(
            "External session aliases are not supported by read-only terminal export",
        ));
    }
    let settings = read_terminal_export_settings()?;
    let session_dir = if let Some(dir) = parsed.session_dir {
        Some(
            crate::utils::paths::normalize_path(JsString::from(dir), &Default::default())
                .map_err(CallbackError::from)?,
        )
    } else if let Some(dir) = pi_js_compat::process_env::get(&crate::config::env_session_dir()) {
        Some(crate::config::expand_tilde_path(&dir)?)
    } else {
        settings.get_session_dir()
    };
    let resolved = resolve_session_path(
        &session.into(),
        &process_cwd().map_err(CallbackError::from)?,
        session_dir.as_ref(),
        None,
    )
    .map_err(CallbackError::from)?;
    let path = match resolved {
        ResolvedSession::Path(path) | ResolvedSession::Local(path) | ResolvedSession::Global { path, .. } => path,
        ResolvedSession::NotFound(arg) => {
            let text = pi_js_compat::js_format!("Error: No session found matching '{}'\n", arg);
            std::io::stderr()
                .write_all(encode_node_utf8(&text).as_bytes())
                .map_err(|error| CallbackError::from(error.to_string()))?;
            return Ok(4);
        }
    };
    let env: BTreeMap<String, String> = std::env::vars().collect();
    let theme = resolve_theme_setting(
        settings.get_theme_setting(),
        detect_terminal_background_from_env(&env).theme,
    );
    init_theme(theme.as_ref(), false).map_err(CallbackError::from)?;
    let output: JsString =
        pi_coding_agent_interactive::export_terminal::export_session_to_terminal(&path, columns as f64)?;
    std::io::stdout()
        .write_all(encode_node_utf8(&output).as_bytes())
        .map_err(|error| CallbackError::from(error.to_string()))?;
    Ok(0)
}
