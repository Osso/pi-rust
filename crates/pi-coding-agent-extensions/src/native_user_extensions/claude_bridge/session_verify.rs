//! Source session-verify.ts: post-write warnings never reject a saved session.
use pi_js_compat::{
    callback_error::CallbackError,
    js_json::{self, JsString, JsValue},
};
use std::path::Path;

pub(super) fn verify_written_session(path: &Path, id: &str, count: usize) -> Result<Vec<JsString>, CallbackError> {
    let path_text: JsString = path
        .to_str()
        .ok_or_else(|| CallbackError::new("Invalid Claude session path"))?
        .into();
    let stat = match std::fs::metadata(path) {
        Ok(stat) => stat,
        Err(error) => {
            return Ok(vec![pi_js_compat::js_format!(
                "file missing after save — path={} err={}",
                path_text,
                pi_js_compat::node_fs::node_fs_error(&error, "stat", &path_text)
            )]);
        }
    };
    let content = match std::fs::read(path) {
        Ok(bytes) => String::from_utf8(bytes).map_err(|e| CallbackError::new(e.to_string()))?,
        Err(error) => {
            return Ok(vec![pi_js_compat::js_format!(
                "file unreadable — path={} size={} err={}",
                path_text,
                stat.len(),
                pi_js_compat::node_fs::node_read_error(&error, &path_text)
            )]);
        }
    };
    let lines = content
        .split('\n')
        .filter(|line| !JsString::from(*line).trim().is_empty())
        .collect::<Vec<_>>();
    if lines.len() != count {
        return Ok(vec![pi_js_compat::js_format!(
            "record count mismatch — expected={} actual={} path={} bytes={}",
            count,
            lines.len(),
            path_text,
            content.encode_utf16().count()
        )]);
    }
    let first = js_json::parse_lossless(lines.first().copied().unwrap_or("undefined"));
    let last = js_json::parse_lossless(lines.last().copied().unwrap_or("undefined"));
    match first.and_then(|first| last.map(|last| (first, last))) {
        Ok((first, last)) => {
            let first = first.get("sessionId");
            let last = last.get("sessionId");
            let matches = |value: Option<&JsValue>| value.and_then(JsValue::as_string).is_some_and(|s| s == id);
            if matches(first) && matches(last) {
                return Ok(Vec::new());
            }
            let show = |value: Option<&JsValue>| {
                value
                    .map(js_json::to_js_string)
                    .transpose()
                    .map_err(CallbackError::new)
                    .map(|s| s.unwrap_or_else(|| "undefined".into()))
            };
            Ok(vec![pi_js_compat::js_format!(
                "sessionId drift — expected={} first={} last={}",
                id,
                show(first)?,
                show(last)?
            )])
        }
        Err(error) => Ok(vec![pi_js_compat::js_format!(
            "malformed JSONL — path={} err={}",
            path_text,
            error.to_string()
        )]),
    }
}

pub(super) fn warning_message(warning: &JsString, cwd: &JsString) -> Result<JsString, CallbackError> {
    let realpath = pi_js_compat::node_fs::realpath_sync_native(cwd)
        .map_err(|error| error.message)
        .unwrap_or_else(|error| pi_js_compat::js_format!("<failed: {}>", error));
    let config = std::env::var("CLAUDE_CONFIG_DIR").unwrap_or_else(|_| "(unset)".into());
    let debug = std::env::var("CLAUDE_BRIDGE_DEBUG").as_deref() == Ok("1");
    let suffix = if debug {
        let agent = crate::config::get_agent_dir().map_err(|e| CallbackError::new(e.to_string()))?;
        let agent =
            Path::new(&agent.to_utf8().map_err(|e| CallbackError::new(e.to_string()))?).join("claude-bridge.log");
        let path = std::env::var("CLAUDE_BRIDGE_DEBUG_PATH")
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or(
                agent
                    .to_str()
                    .ok_or_else(|| CallbackError::new("Invalid bridge log path"))?
                    .into(),
            );
        format!(" and attach {path}")
    } else {
        " (rerun with CLAUDE_BRIDGE_DEBUG=1 to capture a debug log)".into()
    };
    Ok(pi_js_compat::js_format!(
        "Session file issue: {}\ncwd={} realpath={} CLAUDE_CONFIG_DIR={}\nPlease copy and paste this message into a new issue at https://github.com/elidickinson/pi-claude-bridge/issues/new{}",
        warning,
        cwd,
        realpath,
        config,
        suffix
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn claude_bridge_post_write_verification_matches_frozen_ts_warning_cases() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("session.jsonl");
        std::fs::write(
            dir.path().join("session-verify.mjs"),
            include_str!("fixtures/session-verify.mjs"),
        )
        .unwrap();
        for (content, id, count) in [
            (Some("{\"sessionId\":\"id\"}\n"), "id", 1),
            (Some("{\"sessionId\":\"wrong\"}\n"), "id", 1),
            (Some("{\"sessionId\":\"id\",\"text\":\"😀\"}\n"), "id", 2),
            (Some("{broken}\n"), "id", 1),
            (None, "id", 1),
        ] {
            if let Some(content) = content {
                std::fs::write(&path, content).unwrap();
            } else {
                std::fs::remove_file(&path).unwrap();
            }
            let result = verify_written_session(&path, id, count).unwrap();
            let script = "import {verifyWrittenSession} from './session-verify.mjs';process.stdout.write(JSON.stringify(verifyWrittenSession(process.argv[1],process.argv[2],Number(process.argv[3]))));";
            let oracle = std::process::Command::new("node")
                .current_dir(dir.path())
                .args(["--input-type=module", "-e", script])
                .arg(&path)
                .args([id, &count.to_string()])
                .output()
                .unwrap();
            assert!(oracle.status.success(), "{}", String::from_utf8(oracle.stderr).unwrap());
            let expected = js_json::parse_lossless(String::from_utf8(oracle.stdout).unwrap()).unwrap();
            assert_eq!(
                JsValue::Array(result.into_iter().map(JsValue::String).collect()),
                expected
            );
        }
    }
}
