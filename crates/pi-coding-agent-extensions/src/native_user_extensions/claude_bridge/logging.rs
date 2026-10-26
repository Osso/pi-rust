//! Source log-paths.ts and index.ts debug/diagnostic sinks.
use pi_js_compat::{
    callback_error::CallbackError,
    js_json::{self, JsObject, JsString, JsValue},
};
use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

pub(super) fn timestamp() -> Result<String, CallbackError> {
    pi_js_compat::date::to_iso_string_value(pi_js_compat::date::js_now_millis())
        .map_err(|e| CallbackError::new(e.to_string()))
}
pub(super) fn agent_dir() -> Result<PathBuf, CallbackError> {
    let dir = crate::config::get_agent_dir().map_err(|e| CallbackError::new(e.to_string()))?;
    Ok(PathBuf::from(
        dir.to_utf8().map_err(|e| CallbackError::new(e.to_string()))?,
    ))
}
pub(super) fn debug_path() -> Result<PathBuf, CallbackError> {
    match std::env::var_os("CLAUDE_BRIDGE_DEBUG_PATH").filter(|s| !s.is_empty()) {
        Some(path) => Ok(path.into()),
        None => Ok(agent_dir()?.join("claude-bridge.log")),
    }
}
#[derive(Clone)]
pub(super) struct Logs {
    pub module_id: String,
    pub enabled: bool,
    next_cli: Arc<AtomicU64>,
}
impl Logs {
    pub fn new() -> Result<Self, CallbackError> {
        let random = getrandom::u64().map_err(|e| CallbackError::new(e.to_string()))?;
        let digits = b"0123456789abcdefghijklmnopqrstuvwxyz";
        let module_id = (0..6)
            .map(|i| char::from(digits[((random >> (i * 8)) % 36) as usize]))
            .collect();
        Ok(Self {
            module_id,
            enabled: std::env::var("CLAUDE_BRIDGE_DEBUG").as_deref() == Ok("1"),
            next_cli: Arc::new(AtomicU64::new(1)),
        })
    }
    pub fn debug(&self, message: impl Into<JsString>) -> Result<(), CallbackError> {
        if !self.enabled {
            return Ok(());
        }
        append_debug(&debug_path()?, &self.module_id, &timestamp()?, &message.into())
    }
    pub(super) fn debug_error(&self, label: impl Into<JsString>, error: &CallbackError) -> Result<(), CallbackError> {
        self.debug(pi_js_compat::js_format!(
            "{} {}: {}",
            label.into(),
            error.name,
            error.message
        ))
    }
    pub(super) fn debug_final_stream(&self, output: &pi_ai::types::AssistantMessage) -> Result<(), CallbackError> {
        let mut fields = JsObject::new();
        fields.insert(
            "stopReason".into(),
            JsValue::String(stop_reason(output.stop_reason).into()),
        );
        if let Some(error) = &output.error_message {
            fields.insert("error".into(), JsValue::String(error.clone()));
        }
        self.debug(format!(
            "provider: finalizeCurrentStream called, stopReason={}, turnOutput={}",
            stop_reason(output.stop_reason),
            js_json::stringify_lossless(&JsValue::Object(fields))
        ))
    }
    pub fn diag(&self, label: &str, data: JsObject) -> Result<(), CallbackError> {
        let path = agent_dir()?.join("claude-bridge-diag.log");
        let mut entry = JsObject::new();
        entry.insert("ts".into(), JsValue::String(timestamp()?.into()));
        entry.insert(
            "moduleInstanceId".into(),
            JsValue::String(self.module_id.as_str().into()),
        );
        entry.insert("label".into(), JsValue::String(label.into()));
        entry.extend(data);
        append_line(&path, &JsValue::Object(entry))?;
        self.debug(format!("DIAG: {label} (see {})", path.display()))
    }
    pub fn session_warning(&self, path: &Path, cwd: &Path, warning: &JsString) -> Result<(), CallbackError> {
        self.debug(pi_js_compat::js_format!("WARNING session verify: {}", warning))?;
        let text = |path: &Path| {
            path.to_str()
                .map(JsString::from)
                .ok_or_else(|| CallbackError::new("Invalid Claude session path"))
        };
        let cwd = text(cwd)?;
        let realpath = pi_js_compat::node_fs::realpath_sync_native(&cwd)
            .map_err(|e| e.message)
            .unwrap_or_else(|e| pi_js_compat::js_format!("<failed: {}>", e));
        let mut data = JsObject::new();
        data.insert("msg".into(), JsValue::String(warning.clone()));
        data.insert("jsonlPath".into(), JsValue::String(text(path)?));
        data.insert("cwd".into(), JsValue::String(cwd));
        data.insert("realpath".into(), JsValue::String(realpath));
        data.insert(
            "claudeConfigDir".into(),
            std::env::var("CLAUDE_CONFIG_DIR")
                .map(|s| JsValue::String(s.into()))
                .unwrap_or(JsValue::Null),
        );
        self.diag("session_verify_fail", data)
    }
    pub(super) fn diag_empty_prompt(
        &self,
        context: &pi_ai::types::Context,
        reentrant: bool,
        active: usize,
        shared: JsValue,
    ) -> Result<(), CallbackError> {
        let (prompt, images) = prompt_info(context)?;
        if !prompt.is_empty() || images {
            return Ok(());
        }
        let messages = context
            .messages
            .iter()
            .map(|m| js_json::to_value(m).map_err(|e| CallbackError::new(e.to_string())))
            .collect::<Result<Vec<_>, _>>()?;
        let mut data = JsObject::new();
        data.insert("contextLength".into(), JsValue::Number(messages.len() as f64));
        if let Some(last) = messages.last() {
            data.insert(
                "lastMsgRole".into(),
                JsValue::String(super::stream::string(last, "role")),
            );
        }
        data.insert("isReentrant".into(), JsValue::Bool(reentrant));
        data.insert("activeQueryContexts".into(), JsValue::Number(active as f64));
        data.insert("activeQueryExists".into(), JsValue::Bool(reentrant));
        data.insert("sharedSession".into(), shared);
        data.insert(
            "messageRoles".into(),
            JsValue::String(JsString::join(
                messages
                    .iter()
                    .enumerate()
                    .map(|(i, m)| pi_js_compat::js_format!("[{}]{}", i, super::stream::string(m, "role"))),
                " ",
            )),
        );
        self.diag("empty_prompt", data)
    }
    pub(super) fn debug_conversion(
        &self,
        input_count: usize,
        report: &super::convert::ConversionReport,
    ) -> Result<(), CallbackError> {
        let messages = &report.messages;
        self.debug(format!(
            "convertAndImportMessages: {input_count} pi msgs → {} anthropic msgs",
            messages.len()
        ))?;
        let roles = messages.iter().enumerate().map(|(i, message)| {
            let content = message.get("content").unwrap_or(&JsValue::Null);
            let types = if content.as_string().is_some() {
                JsString::from("text")
            } else if let Some(blocks) = content.as_array() {
                JsString::join(blocks.iter().map(|b| super::stream::string(b, "type")), "+")
            } else {
                JsString::from("?")
            };
            pi_js_compat::js_format!("[{}]{}:{}", i, super::stream::string(message, "role"), types)
        });
        self.debug(pi_js_compat::js_format!(
            "convertAndImportMessages: imported roles: {}",
            JsString::join(roles, " ")
        ))?;
        let stats = &report.stats;
        let mut dropped = Vec::new();
        if stats.thinking > 0 {
            let mut providers = stats.providers.iter().cloned().collect::<Vec<_>>();
            providers.sort();
            dropped.push(pi_js_compat::js_format!(
                "{} thinking ({})",
                stats.thinking,
                JsString::join(providers, ", ")
            ));
        }
        if stats.aborted_turns > 0 {
            dropped.push(format!("{} aborted turn(s)", stats.aborted_turns).into());
        }
        dropped.extend(
            stats
                .other
                .iter()
                .map(|(kind, count)| pi_js_compat::js_format!("{} {}", count, kind)),
        );
        if !dropped.is_empty() {
            self.debug(pi_js_compat::js_format!(
                "convertAndImportMessages: dropped {}",
                JsString::join(dropped, ", ")
            ))?;
        }
        if !stats.sanitized_ids.is_empty() {
            let ids = stats.sanitized_ids.iter().map(|(original, clean)| {
                if original == clean {
                    original.clone()
                } else {
                    pi_js_compat::js_format!("{}→{}", original, clean)
                }
            });
            self.debug(pi_js_compat::js_format!(
                "convertAndImportMessages: sanitized {} tool IDs: {}",
                stats.sanitized_ids.len(),
                JsString::join(ids, ", ")
            ))?;
        }
        Ok(())
    }
    pub(super) fn debug_fresh_query(
        &self,
        launch: &super::process::LaunchOptions,
        context: &pi_ai::types::Context,
        context_files: usize,
    ) -> Result<(), CallbackError> {
        let (prompt, images) = prompt_info(context)?;
        let prompt = if prompt.is_empty() && !images {
            JsString::from("[continue]")
        } else {
            prompt
        };
        self.debug(pi_js_compat::js_format!(
            "provider: fresh query model={} msgs={} tools={} resume={} effort={} ctxFiles={} strictMcp={} prompt={}{}",
            launch.model,
            context.messages.len(),
            context.tools.as_ref().map_or(0, Vec::len),
            launch.resume.as_deref().map(|s| &s[..s.len().min(8)]).unwrap_or("none"),
            launch.effort.as_deref().unwrap_or("default"),
            context_files,
            if launch.provider.get("strictMcpConfig").and_then(JsValue::as_bool) == Some(false) {
                "false"
            } else {
                "true"
            },
            prompt.slice(0, 60),
            if images { " [+images]" } else { "" }
        ))
    }
    pub fn debug_session_paths(&self, label: &str, cwd: &Path, path: &Path) -> Result<(), CallbackError> {
        let cwd = cwd
            .to_str()
            .ok_or_else(|| CallbackError::new("Invalid Claude working directory"))?;
        let path_text = path
            .to_str()
            .ok_or_else(|| CallbackError::new("Invalid Claude session path"))?;
        let real = pi_js_compat::node_fs::realpath_sync_native(&cwd.into())
            .map_err(|e| e.message)
            .unwrap_or_else(|error| pi_js_compat::js_format!("<failed: {}>", error));
        let stat = std::fs::metadata(path).ok();
        self.debug(format!("{label}: cwd={cwd}"))?;
        if real != cwd {
            self.debug(pi_js_compat::js_format!(
                "{}: realpath(cwd)={} (DIFFERS — symlink-resolved path is what CC SDK uses)",
                label,
                real
            ))?;
        }
        self.debug(format!("{label}: jsonlPath={path_text}"))?;
        self.debug(format!(
            "{label}: fileExists={}{}",
            stat.is_some(),
            stat.map(|s| format!(" size={}", s.len())).unwrap_or_default()
        ))?;
        self.debug(format!(
            "{label}: env.CLAUDE_CONFIG_DIR={} HOME={}",
            std::env::var("CLAUDE_CONFIG_DIR").unwrap_or_else(|_| "(unset)".into()),
            std::env::var("HOME").unwrap_or_else(|_| "(unset)".into())
        ))
    }
    pub fn cli(&self, tag: &str, profile: Option<super::auth_trace::AuthProfile>) -> Result<CliDebug, CallbackError> {
        let seq = self.next_cli.fetch_add(1, Ordering::Relaxed);
        let debug_file = if self.enabled {
            let dir = debug_path()?
                .parent()
                .ok_or_else(|| CallbackError::new("Invalid bridge debug path"))?
                .join("cc-cli-logs");
            std::fs::create_dir_all(&dir).map_err(|e| CallbackError::new(e.to_string()))?;
            let name = timestamp()?.replace([':', '.'], "-");
            let path = dir.join(format!("{name}-{tag}-{seq}.log"));
            self.debug(format!("cli-debug: {tag} #{seq} → {}", path.display()))?;
            Some(path)
        } else {
            None
        };
        Ok(CliDebug {
            logs: self.clone(),
            tag: tag.into(),
            seq,
            debug_file,
            profile,
            pending: Arc::new(std::sync::Mutex::new(JsString::default())),
        })
    }
}
#[derive(Clone)]
pub(super) struct CliDebug {
    pub(super) logs: Logs,
    tag: String,
    seq: u64,
    pub debug_file: Option<PathBuf>,
    pub profile: Option<super::auth_trace::AuthProfile>,
    pending: Arc<std::sync::Mutex<JsString>>,
}
impl CliDebug {
    pub fn stderr(&self, text: &str) -> Result<(), CallbackError> {
        if let Some(profile) = &self.profile {
            let mut pending = self.pending.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            let combined = pending.clone() + text;
            let mut lines = combined.split('\n').collect::<Vec<_>>();
            let tail = lines.pop().unwrap_or_default();
            for line in lines {
                let line = if line.ends_with("\r") {
                    line.slice(0, line.len() - 1)
                } else {
                    line
                };
                super::auth_trace::record_failure(profile, &line)?;
            }
            if !tail.is_empty() {
                super::auth_trace::record_failure(profile, &tail)?;
            }
            *pending = tail.slice(tail.len().saturating_sub(4096), tail.len());
        }
        for line in text.split(['\n', '\r']).filter(|line| !line.is_empty()) {
            self.logs
                .debug(format!("[cli-stderr {}#{}] {line}", self.tag, self.seq))?;
        }
        Ok(())
    }
}
pub(super) fn stop_reason(reason: pi_ai::types::StopReason) -> &'static str {
    match reason {
        pi_ai::types::StopReason::Stop => "stop",
        pi_ai::types::StopReason::Length => "length",
        pi_ai::types::StopReason::ToolUse => "toolUse",
        pi_ai::types::StopReason::Error => "error",
        pi_ai::types::StopReason::Aborted => "aborted",
    }
}
pub(super) fn prompt_info(context: &pi_ai::types::Context) -> Result<(JsString, bool), CallbackError> {
    let mut turn = context
        .messages
        .iter()
        .rev()
        .take_while(|m| matches!(m, pi_ai::types::Message::User(_)))
        .collect::<Vec<_>>();
    turn.reverse();
    let values = turn
        .iter()
        .map(|m| js_json::to_value(m).map_err(|e| CallbackError::new(e.to_string())))
        .collect::<Result<Vec<_>, _>>()?;
    let texts = values
        .iter()
        .map(|message| {
            let content = message.get("content").unwrap_or(&JsValue::Null);
            if let Some(text) = content.as_string() {
                return text.clone();
            }
            JsString::join(
                content
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter(|b| super::stream::string(b, "type") == "text")
                    .map(|b| super::stream::string(b, "text")),
                "\n",
            )
        })
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>();
    let images = values.iter().any(|m| {
        m.get("content")
            .and_then(JsValue::as_array)
            .is_some_and(|blocks| blocks.iter().any(|b| super::stream::string(b, "type") == "image"))
    });
    Ok((JsString::join(texts, "\n"), images))
}
pub(super) fn append_line(path: &Path, value: &JsValue) -> Result<(), CallbackError> {
    append_text(path, &(js_json::stringify_lossless(value) + "\n"))
}
fn append_text(path: &Path, text: &str) -> Result<(), CallbackError> {
    use std::io::Write;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| CallbackError::new(e.to_string()))?;
    }
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .and_then(|mut file| file.write_all(text.as_bytes()))
        .map_err(|e| CallbackError::new(e.to_string()))
}
fn append_debug(path: &Path, module: &str, time: &str, message: &JsString) -> Result<(), CallbackError> {
    append_text(
        path,
        &format!("[{time}] [{module}] {}\n", js_json::encode_node_utf8(message)),
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn claude_bridge_debug_log_keeps_source_prefix_and_node_utf8_encoding() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bridge.log");
        let message = JsString::from_utf16(vec![65, 0xd800]);
        append_debug(&path, "abc123", "2026-10-25T12:00:00.000Z", &message).unwrap();
        assert_eq!(
            std::fs::read_to_string(path).unwrap(),
            "[2026-10-25T12:00:00.000Z] [abc123] A�\n"
        );
    }
}
