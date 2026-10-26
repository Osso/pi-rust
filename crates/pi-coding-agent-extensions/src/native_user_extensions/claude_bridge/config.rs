//! Source config.ts: shallow nested override, lossless user settings and notice marker.
use std::path::Path;

use pi_js_compat::callback_error::CallbackError;
use pi_js_compat::js_json::{self, JsObject, JsString, JsValue};

pub fn merge_config(global: &JsValue, project: &JsValue) -> JsValue {
    let mut merged = JsObject::new();
    let notice = project
        .get("startupNoticeShown")
        .filter(|v| !matches!(v, JsValue::Null))
        .or_else(|| global.get("startupNoticeShown"));
    if let Some(notice) = notice {
        merged.insert("startupNoticeShown".into(), notice.clone());
    }
    for section in ["askClaude", "provider"] {
        let mut fields = global
            .get(section)
            .and_then(JsValue::as_object)
            .cloned()
            .unwrap_or_default();
        if let Some(overrides) = project.get(section).and_then(JsValue::as_object) {
            fields.extend(overrides.clone());
        }
        merged.insert(section.into(), JsValue::Object(fields));
    }
    JsValue::Object(merged)
}

pub fn load_config(agent_dir: &Path, cwd: &Path, config_dir_name: &str) -> JsValue {
    merge_config(
        &read_config(&agent_dir.join("claude-bridge.json")),
        &read_config(&cwd.join(config_dir_name).join("claude-bridge.json")),
    )
}

fn read_config(path: &Path) -> JsValue {
    match std::fs::read(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => JsValue::Object(JsObject::new()),
        result => match result.map_err(|e| e.to_string()).and_then(|bytes| {
            js_json::parse_lossless(String::from_utf8_lossy(&bytes).as_ref()).map_err(|e| e.to_string())
        }) {
            Ok(value) => value,
            Err(error) => {
                eprintln!("claude-bridge: failed to parse {}: {error}", path.display());
                JsValue::Object(JsObject::new())
            }
        },
    }
}

pub fn write_startup_notice(agent_dir: &Path, date: &str) -> Result<JsString, CallbackError> {
    let path = agent_dir.join("claude-bridge.json");
    let path_text: JsString = path
        .as_os_str()
        .to_str()
        .ok_or_else(|| CallbackError::new("Invalid config path"))?
        .into();
    let mut existing = match std::fs::read(&path) {
        Ok(bytes) => match js_json::parse_lossless(String::from_utf8_lossy(&bytes).as_ref()) {
            Ok(JsValue::Object(object)) => object,
            Ok(_) => JsObject::new(),
            Err(error) => {
                eprintln!(
                    "claude-bridge: leaving {} alone, it does not parse: {error}",
                    path.display()
                );
                return Ok(path_text);
            }
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => JsObject::new(),
        Err(error) => {
            eprintln!(
                "claude-bridge: leaving {} alone, it does not parse: {error}",
                path.display()
            );
            return Ok(path_text);
        }
    };
    existing.insert("startupNoticeShown".into(), JsValue::String(date.into()));
    std::fs::create_dir_all(agent_dir).map_err(|e| CallbackError::new(e.to_string()))?;
    let json = js_json::stringify_lossless_pretty(&JsValue::Object(existing));
    std::fs::write(&path, format!("{json}\n")).map_err(|e| CallbackError::new(e.to_string()))?;
    Ok(path_text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_bridge_config_project_fields_override_without_losing_global_settings() {
        let global = js_json::parse_lossless(r#"{"startupNoticeShown":"2026-10-01","askClaude":{"enabled":true,"description":"\ud800"},"provider":{"plan":"max","strictMcpConfig":true}}"#).unwrap();
        let project =
            js_json::parse_lossless(r#"{"askClaude":{"defaultMode":"none"},"provider":{"strictMcpConfig":false}}"#)
                .unwrap();
        let expected = js_json::parse_lossless(r#"{"startupNoticeShown":"2026-10-01","askClaude":{"enabled":true,"description":"\ud800","defaultMode":"none"},"provider":{"plan":"max","strictMcpConfig":false}}"#).unwrap();
        assert_eq!(merge_config(&global, &project), expected);
    }

    #[test]
    fn claude_bridge_notice_preserves_unknown_utf16_and_does_not_destroy_invalid_config() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("claude-bridge.json");
        std::fs::write(&path, "{trailing,}").unwrap();
        write_startup_notice(dir.path(), "2026-10-25").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "{trailing,}");
        std::fs::write(&path, r#"{"other":"\udfff","provider":{"plan":"max"}}"#).unwrap();
        write_startup_notice(dir.path(), "2026-10-25").unwrap();
        let actual = std::fs::read_to_string(&path).unwrap();
        assert_eq!(
            actual,
            "{\n  \"other\": \"\\udfff\",\n  \"provider\": {\n    \"plan\": \"max\"\n  },\n  \"startupNoticeShown\": \"2026-10-25\"\n}\n"
        );
    }
}
