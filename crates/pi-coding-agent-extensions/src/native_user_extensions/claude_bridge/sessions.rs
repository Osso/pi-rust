//! One Claude mirror per Pi session; rebuilds use Pi history as authority.

use super::{
    convert, native_prefix,
    session_records::{self, RecordWriter},
};
use pi_ai::types::{Context, Message};
use pi_js_compat::callback_error::CallbackError;
use pi_js_compat::js_json::{self, JsString, JsValue};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

fn association_io_error(error: &std::io::Error, syscall: &str, path: &Path) -> CallbackError {
    CallbackError::new(pi_js_compat::node_fs::node_fs_error(
        error,
        syscall,
        &pi_js_compat::node_fs::decode_node_path(path.as_os_str()),
    ))
}

struct Mirror {
    id: String,
    cursor: usize,
    rebuild: bool,
    rotate: bool,
}

pub struct SessionStore {
    pub claude_dir: PathBuf,
    agent_dir: PathBuf,
    mirrors: HashMap<JsString, Mirror>,
    pub verification_logs: Option<super::logging::Logs>,
    pub verification_ui: Option<Arc<dyn crate::core::extensions::types::ExtensionUiContext>>,
}

impl SessionStore {
    pub fn from_environment() -> Result<Self, CallbackError> {
        let claude_dir = match std::env::var_os("CLAUDE_CONFIG_DIR") {
            Some(dir) => PathBuf::from(dir),
            None => PathBuf::from(
                pi_js_compat::node_os::homedir()
                    .map_err(|e| CallbackError::new(e.to_string()))?
                    .to_utf8()
                    .map_err(|e| CallbackError::new(e.to_string()))?,
            )
            .join(".claude"),
        };
        let agent_dir = PathBuf::from(
            crate::config::get_agent_dir()
                .map_err(|e| CallbackError::new(e.to_string()))?
                .to_utf8()
                .map_err(|e| CallbackError::new(e.to_string()))?,
        );
        Ok(Self::at(claude_dir, agent_dir))
    }
    pub fn at(claude_dir: PathBuf, agent_dir: PathBuf) -> Self {
        Self {
            claude_dir,
            agent_dir,
            mirrors: HashMap::new(),
            verification_ui: None,
            verification_logs: None,
        }
    }

    fn association_path(&self, key: &JsString) -> Result<PathBuf, CallbackError> {
        let encoded = pi_js_compat::string::encode_uri_component(key).map_err(|e| CallbackError::new(e.to_string()))?;
        Ok(self
            .agent_dir
            .join("claude-bridge-sessions")
            .join(format!("{encoded}.json")))
    }
    fn recorded(&self, key: &JsString, cwd: &Path) -> Result<Option<String>, CallbackError> {
        if key == "(none)" {
            return Ok(None);
        }
        let path = self.association_path(key)?;
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => {
                let path = path
                    .to_str()
                    .map(JsString::from)
                    .ok_or_else(|| CallbackError::new("Invalid Claude association path"))?;
                return Err(CallbackError::new(pi_js_compat::node_fs::node_read_error(
                    &error, &path,
                )));
            }
        };
        let text = std::str::from_utf8(&bytes).map_err(|e| CallbackError::new(e.to_string()))?;
        let value = js_json::parse_lossless(text).map_err(|error| {
            let mut error = CallbackError::new(error.into_message());
            error.name = "SyntaxError".into();
            error
        })?;
        if value
            .get("cwd")
            .and_then(JsValue::as_string)
            .is_some_and(|v| cwd.to_str().is_some_and(|cwd| v == cwd))
        {
            value
                .get("sessionId")
                .and_then(JsValue::as_string)
                .map(|s| s.to_utf8().map_err(|e| CallbackError::new(e.to_string())))
                .transpose()
        } else {
            Ok(None)
        }
    }
    fn read_recorded_session(&self, key: &JsString, cwd: &Path) -> Result<Option<String>, CallbackError> {
        match self.recorded(key, cwd) {
            Ok(id) => Ok(id),
            Err(error) => {
                if let Some(logs) = &self.verification_logs {
                    logs.debug_error(
                        pi_js_compat::js_format!(
                            "WARNING: could not read the recorded session for pi session {}:",
                            key
                        ),
                        &error,
                    )?;
                }
                Ok(None)
            }
        }
    }

    fn read_previous_session(&self, id: &str, cwd: &Path) -> Result<Vec<JsValue>, CallbackError> {
        let records = session_records::session_path(&self.claude_dir, cwd, id)
            .and_then(|path| session_records::read_records(&path));
        match records {
            Ok(records) => Ok(records),
            Err(error) => {
                if let Some(logs) = &self.verification_logs {
                    logs.debug_error(
                        pi_js_compat::js_format!("WARNING: could not read session {}:", JsString::from(id).slice(0, 8)),
                        &error,
                    )?;
                }
                Ok(Vec::new())
            }
        }
    }

    pub fn record(&mut self, key: &JsString, id: String, cursor: usize, cwd: &Path) -> Result<(), CallbackError> {
        self.mirrors.insert(
            key.clone(),
            Mirror {
                id: id.clone(),
                cursor,
                rebuild: false,
                rotate: false,
            },
        );
        if key == "(none)" {
            return Ok(());
        }
        if let Err(error) = self.write_association(key, &id, cwd)
            && let Some(logs) = &self.verification_logs
        {
            logs.debug_error(
                pi_js_compat::js_format!(
                    "WARNING: could not record session {} for pi session {}:",
                    JsString::from(id).slice(0, 8),
                    key
                ),
                &error,
            )?;
        }
        Ok(())
    }

    fn write_association(&self, key: &JsString, id: &str, cwd: &Path) -> Result<(), CallbackError> {
        let cwd = cwd
            .to_str()
            .ok_or_else(|| CallbackError::new("Invalid Claude working directory"))?;
        let path = self.association_path(key)?;
        let parent = path
            .parent()
            .ok_or_else(|| CallbackError::new("Invalid Claude association path"))?;
        std::fs::create_dir_all(parent).map_err(|error| association_io_error(&error, "mkdir", parent))?;
        let temp = path.with_extension(format!("json.{}.tmp", std::process::id()));
        let content = JsValue::object([
            ("sessionId", JsValue::String(id.into())),
            ("cwd", JsValue::String(cwd.into())),
        ]);
        std::fs::write(&temp, js_json::stringify_lossless(&content))
            .map_err(|error| association_io_error(&error, "open", &temp))?;
        std::fs::rename(&temp, &path).map_err(|error| {
            CallbackError::new(pi_js_compat::node_fs::node_fs_dest_error(
                &error,
                "rename",
                &pi_js_compat::node_fs::decode_node_path(temp.as_os_str()),
                &pi_js_compat::node_fs::decode_node_path(path.as_os_str()),
            ))
        })
    }
    pub fn complete(&mut self, key: &JsString, id: String, cursor: usize, cwd: &Path) -> Result<(), CallbackError> {
        let old = self.mirrors.get(key);
        let cursor = old.map_or(cursor, |m| m.cursor.max(cursor));
        if let Some(logs) = &self.verification_logs {
            logs.debug(pi_js_compat::js_format!(
                "provider: query done, session={}, cursor={}",
                super::logging::truncate_session_id(&id),
                cursor
            ))?;
        }
        let rebuild = old.is_some_and(|m| m.rebuild);
        let rotate = old.is_some_and(|m| m.rotate);
        self.record(key, id, cursor, cwd)?;
        if rebuild {
            self.mark_rebuild(key, rotate);
        }
        Ok(())
    }

    pub fn seed_live_session(&mut self, key: &JsString, id: String) {
        self.mirrors.entry(key.clone()).or_insert(Mirror {
            id,
            cursor: 0,
            rebuild: true,
            rotate: true,
        });
    }

    pub fn advance_cursor(&mut self, key: &JsString, cursor: usize) {
        if let Some(mirror) = self.mirrors.get_mut(key) {
            mirror.cursor = cursor;
        }
    }

    pub fn preserve_unowned(&self, key: &JsString, context: &Context) -> bool {
        let prior = context
            .messages
            .iter()
            .rev()
            .take_while(|m| matches!(m, Message::User(_)))
            .count();
        let count = context.messages.len() - prior;
        key == "(none)" && self.mirrors.get(key).is_some_and(|m| !m.rebuild && count < m.cursor)
    }

    pub fn delete_ephemeral(&self, key: &JsString, id: &str, cwd: &Path) -> Result<(), CallbackError> {
        if self.mirrors.get(key).is_some_and(|mirror| mirror.id == id) {
            if let Some(logs) = &self.verification_logs {
                logs.debug(pi_js_compat::js_format!(
                    "provider: query done, ignoring captured session {} to preserve shared session",
                    super::logging::truncate_session_id(id)
                ))?;
            }
            return Ok(());
        }
        let path = session_records::session_path(&self.claude_dir, cwd, id)?;
        match std::fs::remove_file(&path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(CallbackError::new(e.to_string())),
        }
        if let Some(logs) = &self.verification_logs {
            logs.debug(pi_js_compat::js_format!(
                "provider: query done, deleted ephemeral session {} to preserve shared session",
                super::logging::truncate_session_id(id)
            ))?;
            logs.debug(pi_js_compat::js_format!(
                "provider: query done, ignoring captured session {} to preserve shared session",
                super::logging::truncate_session_id(id)
            ))?;
        }
        let companion = path.with_extension("");
        match std::fs::remove_dir_all(companion) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(CallbackError::new(e.to_string())),
        }
    }

    pub fn sync_ask(
        &mut self,
        key: &JsString,
        context: &Context,
        cwd: &Path,
        model: &str,
    ) -> Result<Option<String>, CallbackError> {
        if let Some(mirror) = self.mirrors.get(key) {
            return Ok(Some(mirror.id.clone()));
        }
        self.sync(key, context, cwd, model, false)
    }

    pub(super) fn mirror_id(&self, key: &JsString) -> Option<&str> {
        self.mirrors.get(key).map(|m| m.id.as_str())
    }
    pub(super) fn mirror_count(&self) -> usize {
        self.mirrors.len()
    }
    pub fn clear(&mut self) {
        self.mirrors.clear();
    }

    #[cfg(test)]
    pub(super) fn needs_rebuild(&self, key: &JsString) -> bool {
        self.mirrors.get(key).is_some_and(|m| m.rebuild)
    }
    pub(super) fn debug_shared_session(&self, key: &JsString) -> JsValue {
        self.mirrors
            .get(key)
            .or_else(|| self.mirrors.get(&JsString::from("(none)")))
            .map_or(JsValue::Null, |m| {
                JsValue::object([
                    ("sessionId", JsValue::String(super::logging::truncate_session_id(&m.id))),
                    ("cursor", JsValue::Number(m.cursor as f64)),
                ])
            })
    }
    pub fn mark_rebuild(&mut self, key: &JsString, rotate: bool) {
        if let Some(mirror) = self.mirrors.get_mut(key) {
            mirror.rebuild = true;
            mirror.rotate |= rotate;
        }
    }

    pub fn sync(
        &mut self,
        key: &JsString,
        context: &Context,
        cwd: &Path,
        model: &str,
        provider: bool,
    ) -> Result<Option<String>, CallbackError> {
        let mut start = context.messages.len();
        while start > 0 && matches!(context.messages[start - 1], Message::User(_)) {
            start -= 1;
        }
        let prior = &context.messages[..start];
        let mirror = self.mirrors.get(key);
        if let Some(mirror) = mirror.filter(|m| !m.rebuild && prior.len() >= m.cursor) {
            let missed = &prior[mirror.cursor..];
            if missed.is_empty() || matches!(missed, [Message::Assistant(_)]) {
                let id = mirror.id.clone();
                if let Some(logs) = &self.verification_logs {
                    logs.debug(if missed.is_empty() {
                        pi_js_compat::js_format!(
                            "Case 3: resuming session {}, cursor={}",
                            super::logging::truncate_session_id(&id),
                            mirror.cursor
                        )
                    } else {
                        pi_js_compat::js_format!(
                            "Case 3: advanced cursor past trailing assistant, resuming session {}, cursor={}",
                            super::logging::truncate_session_id(&id),
                            prior.len()
                        )
                    })?;
                }
                if let Some(logs) = &self.verification_logs {
                    logs.debug(format!(
                        "syncResult: path=reuse sessionId={} cursor={}",
                        id,
                        prior.len()
                    ))?;
                }
                if !missed.is_empty()
                    && let Some(mirror) = self.mirrors.get_mut(key)
                {
                    mirror.cursor = prior.len();
                }
                return Ok(Some(id));
            }
        }
        if key == "(none)" && mirror.is_some_and(|m| !m.rebuild && prior.len() < m.cursor) {
            if let (Some(logs), Some(mirror)) = (&self.verification_logs, mirror) {
                logs.debug(pi_js_compat::js_format!(
                    "Case 1 synthetic: clean start for shorter context, preserving shared session {}, cursor={}",
                    super::logging::truncate_session_id(&mirror.id),
                    mirror.cursor
                ))?;
                logs.debug(format!(
                    "syncResult: path=clean-start preserve-shared sessionId={} cursor={}",
                    mirror.id, mirror.cursor
                ))?;
            }
            return Ok(None);
        }
        if prior.is_empty() {
            if let Some(logs) = &self.verification_logs {
                logs.debug(format!(
                    "Case 1: clean start, {} total messages",
                    context.messages.len()
                ))?;
                logs.debug("syncResult: path=clean-start")?;
            }
            return Ok(None);
        }
        let old_id = match mirror {
            Some(mirror) => Some(mirror.id.clone()),
            None => self.read_recorded_session(key, cwd)?,
        };
        let native = old_id
            .as_ref()
            .map(|id| self.read_previous_session(id, cwd))
            .transpose()?
            .unwrap_or_default();
        let id = match mirror.filter(|m| !m.rotate) {
            Some(m) => m.id.clone(),
            None => session_records::new_id()?,
        };
        let report = convert::convert_messages_report(prior, provider)?;
        if let Some(logs) = &self.verification_logs {
            logs.debug_conversion(prior.len(), &report)?;
        }
        let converted = super::repair::repair_tool_pairing(&report.messages);
        if converted.len() != report.messages.len()
            && let Some(logs) = &self.verification_logs
        {
            logs.debug(format!(
                "convertAndImportMessages: repairToolPairing {} → {} msgs",
                report.messages.len(),
                converted.len()
            ))?;
        }
        let (mut prefix, covered) = native_prefix::native_prefix(&native, &converted);
        if !native.is_empty()
            && let Some(logs) = &self.verification_logs
        {
            logs.debug(format!(
                "convertAndImportMessages: kept {} of {} CC records, covering {}/{} msgs",
                prefix.len(),
                native.len(),
                covered,
                converted.len()
            ))?;
        }
        let (attachments, skipped) = native_prefix::tail_attachments(&native, &prefix, &converted, covered);
        if !skipped.is_empty()
            && let Some(logs) = &self.verification_logs
        {
            logs.debug(pi_js_compat::js_format!(
                "convertAndImportMessages: dropped {} carried attachment(s): {}",
                skipped.len(),
                JsString::join(skipped, "; ")
            ))?;
        }
        if !attachments.is_empty()
            && let Some(logs) = &self.verification_logs
        {
            logs.debug(format!(
                "convertAndImportMessages: carrying {} attachment(s) across the rebuild",
                attachments.values().map(Vec::len).sum::<usize>()
            ))?;
        }
        for record in &mut prefix {
            if let JsValue::Object(fields) = record {
                fields.insert("sessionId".into(), JsValue::String(id.as_str().into()));
            }
        }
        let mut writer = RecordWriter::new(&id, cwd, model, prefix)?;
        for (index, message) in converted[covered..].iter().enumerate() {
            writer.import_message(message)?;
            for attachment in attachments.get(&index).into_iter().flatten() {
                writer.attachment(attachment.clone())?;
            }
        }
        let path = session_records::session_path(&self.claude_dir, cwd, &id)?;
        if path.exists() {
            std::fs::remove_file(&path).map_err(|e| CallbackError::new(e.to_string()))?;
            let companion = path.with_extension("");
            if companion.exists() {
                std::fs::remove_dir_all(companion).map_err(|e| CallbackError::new(e.to_string()))?;
            }
        }
        session_records::write_records(&path, &writer.records)?;
        for warning in super::session_verify::verify_written_session(&path, &id, writer.records.len())? {
            if let Some(logs) = &self.verification_logs {
                logs.session_warning(&path, cwd, &warning)?;
            }
            if let Some(ui) = &self.verification_ui {
                let cwd: JsString = cwd
                    .to_str()
                    .ok_or_else(|| CallbackError::new("Invalid Claude working directory"))?
                    .into();
                ui.notify(
                    &super::session_verify::warning_message(&warning, &cwd)?,
                    Some(crate::core::extensions::types::NotifyType::Warning),
                )?;
            }
        }
        if let Some(logs) = &self.verification_logs {
            match mirror {
                None => logs.debug(pi_js_compat::js_format!("Case 2: first turn with {} prior messages → session {}{}, {} records", prior.len(), super::logging::truncate_session_id(&id), old_id.as_ref().map(|old| pi_js_compat::js_format!(" seeded from recorded session {}", super::logging::truncate_session_id(old))).unwrap_or_default(), writer.records.len()))?,
                Some(m) if !m.rotate => logs.debug(pi_js_compat::js_format!("Case 4: {} missed messages, {} total → rewrote session {} (same id), {} records", (prior.len() as i64 - m.cursor as i64), prior.len(), super::logging::truncate_session_id(&id), writer.records.len()))?,
                Some(m) => logs.debug(pi_js_compat::js_format!("Case 4 post-abort: {} total → new session {} (was {}, rotated to avoid race with orphan writer), {} records", prior.len(), super::logging::truncate_session_id(&id), super::logging::truncate_session_id(&m.id), writer.records.len()))?,
            }
            logs.debug_session_paths(&super::logging::truncate_session_id(&id), cwd, &path)?;
        }
        if let Some(logs) = &self.verification_logs {
            logs.debug(format!(
                "syncResult: path=rebuild sessionId={} priors={} {}",
                id,
                prior.len(),
                if mirror.is_none() {
                    "first"
                } else if mirror.is_some_and(|m| !m.rotate) {
                    "preserved"
                } else {
                    "rotated-post-abort"
                }
            ))?;
        }
        self.record(key, id.clone(), prior.len(), cwd)?;
        Ok(Some(id))
    }
}

#[cfg(test)]
mod persistence_read_tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn run_isolated(name: &str, association_write: bool) -> Option<tempfile::TempDir> {
        if std::env::var_os("BRIDGE_IO_TEST_ROOT").is_some() {
            return None;
        }
        let dir = tempfile::tempdir().unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", name.split_once("::").unwrap().1, "--nocapture"])
            .env("BRIDGE_IO_TEST_ROOT", dir.path())
            .env("BRIDGE_IO_TEST_WRITE", if association_write { "1" } else { "0" })
            .env("CLAUDE_BRIDGE_DEBUG", "1")
            .env("CLAUDE_BRIDGE_DEBUG_PATH", dir.path().join("debug.log"))
            .output()
            .unwrap();
        assert!(
            String::from_utf8(output.stdout.clone())
                .unwrap()
                .contains("running 1 test"),
            "{output:?}"
        );
        assert!(output.status.success(), "{output:?}");
        Some(dir)
    }

    async fn sync_and_check_warning(unreadable: bool) {
        let root = PathBuf::from(std::env::var_os("BRIDGE_IO_TEST_ROOT").unwrap());
        let mut store = SessionStore::at(root.join("cc"), root.join("agent"));
        store.verification_logs = Some(super::super::logging::Logs::new().unwrap());
        let key = JsString::from("pi-😀");
        let association_write = std::env::var("BRIDGE_IO_TEST_WRITE").as_deref() == Ok("1");
        if association_write {
            let parent = store.association_path(&key).unwrap().parent().unwrap().to_path_buf();
            std::fs::create_dir_all(&parent).unwrap();
            std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o500)).unwrap();
        }
        let old_id = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
        let expected = if association_write {
            String::new()
        } else if unreadable {
            store.record(&key, old_id.into(), 0, &root).unwrap();
            // The in-memory mirror takes precedence; this file must not be read.
            std::fs::write(store.association_path(&key).unwrap(), "{").unwrap();
            let path = session_records::session_path(&store.claude_dir, &root, old_id).unwrap();
            session_records::write_records(&path, &[]).unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o0)).unwrap();
            assert_eq!(
                std::fs::read(&path).unwrap_err().kind(),
                std::io::ErrorKind::PermissionDenied
            );
            store.mark_rebuild(&key, true);
            format!(
                "WARNING: could not read session aaaaaaaa: Error: EACCES: permission denied, open '{}'",
                path.to_str().unwrap()
            )
        } else {
            let path = store.association_path(&key).unwrap();
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, "{").unwrap();
            "WARNING: could not read the recorded session for pi session pi-😀: SyntaxError: Expected property name or '}' in JSON at position 1 (line 1 column 2)".into()
        };
        let messages = js_json::parse_lossless(
            r#"[{"role":"user","content":"prior","timestamp":1},{"role":"assistant","api":"anthropic-messages","provider":"anthropic","model":"m","content":[{"type":"text","text":"reply"}],"usage":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0,"totalTokens":0,"cost":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0,"total":0}},"stopReason":"stop","timestamp":2},{"role":"user","content":"next","timestamp":3}]"#,
        ).unwrap();
        let context = Context {
            system_prompt: None,
            tools: None,
            messages: js_json::from_value(&messages).unwrap(),
        };
        let id = store
            .sync(&key, &context, &root, "m", true)
            .expect("query preparation must proceed")
            .unwrap();
        assert_ne!(id, old_id);
        let path = session_records::session_path(&store.claude_dir, &root, &id).unwrap();
        assert_eq!(session_records::read_records(&path).unwrap().len(), 2);

        let (peer, config, _) = super::super::integration_tests::setup().await;
        let bridge = super::super::provider::BridgeProvider::new(
            JsValue::Null,
            Ok(peer.path().join("claude")),
            "AskClaude".into(),
        );
        *bridge.sessions.lock().unwrap() = Ok(store);
        let mut options = pi_ai::request::SimpleStreamOptions::default();
        options.stream.cwd = Some(root.to_str().unwrap().into());
        options.stream.session_id = Some(key.clone());
        let output = bridge
            .stream(super::super::integration_tests::model(&config), context, Some(options))
            .unwrap()
            .result()
            .await
            .unwrap();
        assert_eq!(
            output.stop_reason,
            pi_ai::types::StopReason::Stop,
            "{:?}",
            output.error_message
        );
        let pi_ai::types::AssistantContent::Text(text) = &output.content[0] else {
            panic!("expected query response");
        };
        assert_eq!(text.text, "Claude: next");
        let log = std::fs::read_to_string(root.join("debug.log")).unwrap();
        let warnings: Vec<_> = log
            .lines()
            .filter_map(|line| line.find("WARNING:").map(|i| &line[i..]))
            .collect();
        if association_write {
            let temp = root
                .join("agent/claude-bridge-sessions/pi-%F0%9F%98%80.json")
                .with_extension(format!("json.{}.tmp", std::process::id()));
            let sessions = bridge.sessions.lock().unwrap();
            let completed_id = &sessions.as_ref().unwrap().mirrors.get(&key).unwrap().id;
            let expected: Vec<_> = [&id, completed_id].into_iter().map(|id| format!(
                "WARNING: could not record session {} for pi session pi-😀: Error: EACCES: permission denied, open '{}'",
                &id[..8], temp.to_str().unwrap()
            )).collect();
            assert_eq!(warnings, expected);
            return;
        }
        assert_eq!(warnings, [expected]);
    }

    #[test]
    fn prior_session_missing_and_torn_lines_keep_existing_behavior() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("prior.jsonl");
        assert!(session_records::read_records(&path).unwrap().is_empty());
        std::fs::write(&path, "{\"uuid\":\"kept\"}\n{\"uuid\":").unwrap();
        assert_eq!(
            session_records::read_records(&path).unwrap(),
            vec![JsValue::object([("uuid", JsValue::String("kept".into()))])]
        );
    }

    #[tokio::test]
    async fn unreadable_prior_session_warns_and_query_proceeds() {
        if run_isolated(
            concat!(module_path!(), "::unreadable_prior_session_warns_and_query_proceeds"),
            false,
        )
        .is_some()
        {
            return;
        }
        sync_and_check_warning(true).await;
    }

    #[tokio::test]
    async fn unwritable_association_warns_and_query_proceeds() {
        if run_isolated(
            concat!(module_path!(), "::unwritable_association_warns_and_query_proceeds"),
            true,
        )
        .is_some()
        {
            return;
        }
        sync_and_check_warning(false).await;
    }

    #[tokio::test]
    async fn malformed_association_warns_and_query_proceeds() {
        if run_isolated(
            concat!(module_path!(), "::malformed_association_warns_and_query_proceeds"),
            false,
        )
        .is_some()
        {
            return;
        }
        sync_and_check_warning(false).await;
    }
}
