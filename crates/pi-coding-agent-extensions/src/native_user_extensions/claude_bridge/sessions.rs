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
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(CallbackError::new(e.to_string())),
        };
        let text = std::str::from_utf8(&bytes).map_err(|e| CallbackError::new(e.to_string()))?;
        let value = js_json::parse_lossless(text).map_err(|e| CallbackError::new(e.to_string()))?;
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
        let cwd = cwd
            .to_str()
            .ok_or_else(|| CallbackError::new("Invalid Claude working directory"))?;
        let path = self.association_path(key)?;
        let parent = path
            .parent()
            .ok_or_else(|| CallbackError::new("Invalid Claude association path"))?;
        std::fs::create_dir_all(parent).map_err(|e| CallbackError::new(e.to_string()))?;
        let temp = path.with_extension(format!("json.{}.tmp", std::process::id()));
        let content = JsValue::object([
            ("sessionId", JsValue::String(id.into())),
            ("cwd", JsValue::String(cwd.into())),
        ]);
        std::fs::write(&temp, js_json::stringify_lossless(&content)).map_err(|e| CallbackError::new(e.to_string()))?;
        std::fs::rename(temp, path).map_err(|e| CallbackError::new(e.to_string()))
    }
    pub fn complete(&mut self, key: &JsString, id: String, cursor: usize, cwd: &Path) -> Result<(), CallbackError> {
        let old = self.mirrors.get(key);
        let cursor = old.map_or(cursor, |m| m.cursor.max(cursor));
        if let Some(logs) = &self.verification_logs {
            logs.debug(format!(
                "provider: query done, session={}, cursor={}",
                &id[..id.len().min(8)],
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
                logs.debug(format!(
                    "provider: query done, ignoring captured session {} to preserve shared session",
                    &id[..id.len().min(8)]
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
            logs.debug(format!(
                "provider: query done, deleted ephemeral session {} to preserve shared session",
                &id[..id.len().min(8)]
            ))?;
            logs.debug(format!(
                "provider: query done, ignoring captured session {} to preserve shared session",
                &id[..id.len().min(8)]
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

    pub fn clear(&mut self) {
        self.mirrors.clear();
    }

    pub(super) fn debug_shared_session(&self, key: &JsString) -> JsValue {
        self.mirrors
            .get(key)
            .or_else(|| self.mirrors.get(&JsString::from("(none)")))
            .map_or(JsValue::Null, |m| {
                JsValue::object([
                    ("sessionId", JsValue::String(m.id[..m.id.len().min(8)].into())),
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
                        format!(
                            "Case 3: resuming session {}, cursor={}",
                            &id[..id.len().min(8)],
                            mirror.cursor
                        )
                    } else {
                        format!(
                            "Case 3: advanced cursor past trailing assistant, resuming session {}, cursor={}",
                            &id[..id.len().min(8)],
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
                logs.debug(format!(
                    "Case 1 synthetic: clean start for shorter context, preserving shared session {}, cursor={}",
                    &mirror.id[..mirror.id.len().min(8)],
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
        let old_id = mirror.map(|m| m.id.clone()).or(self.recorded(key, cwd)?);
        let native = old_id
            .as_ref()
            .map(|id| {
                session_records::session_path(&self.claude_dir, cwd, id)
                    .and_then(|path| session_records::read_records(&path))
            })
            .transpose()?
            .unwrap_or_default();
        let id = match mirror.filter(|m| !m.rotate) {
            Some(m) => m.id.clone(),
            None => session_records::new_id()?,
        };
        let converted = convert::convert_messages(prior, provider)?;
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
                None => logs.debug(format!("Case 2: first turn with {} prior messages → session {}{}, {} records", prior.len(), &id[..id.len().min(8)], old_id.as_ref().map(|old| format!(" seeded from recorded session {}", &old[..old.len().min(8)])).unwrap_or_default(), writer.records.len()))?,
                Some(m) if !m.rotate => logs.debug(format!("Case 4: {} missed messages, {} total → rewrote session {} (same id), {} records", prior.len().saturating_sub(m.cursor), prior.len(), &id[..id.len().min(8)], writer.records.len()))?,
                Some(m) => logs.debug(format!("Case 4 post-abort: {} total → new session {} (was {}, rotated to avoid race with orphan writer), {} records", prior.len(), &id[..id.len().min(8)], &m.id[..m.id.len().min(8)], writer.records.len()))?,
            }
            logs.debug_session_paths(&id[..id.len().min(8)], cwd, &path)?;
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
