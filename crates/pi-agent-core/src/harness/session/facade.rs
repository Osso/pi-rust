//! Frozen session/session.ts facade; no lanes, sequence numbers or retained-tail records.

use super::context::{SessionContext, build_session_context};
use super::types::*;
use crate::agent::AgentMessage;
use crate::harness::messages::CustomMessageContent;
use pi_js_compat::js_json::{JsString, JsValue};
use pi_js_compat::js_regex::JsRegex;
use std::sync::Arc;

#[derive(Debug, Clone, Default)]
pub struct CompactionAppendOptions {
    pub details: Option<JsValue>,
    pub from_hook: Option<bool>,
    pub duration_ms: Option<f64>,
}
#[derive(Debug, Clone)]
pub struct MoveSummary {
    pub summary: JsString,
    pub details: Option<JsValue>,
    pub from_hook: Option<bool>,
}

#[derive(Clone)]
pub struct Session<M: SessionMetadataValue = SessionMetadata> {
    storage: Arc<dyn SessionStorage<M>>,
    clock: Clock,
}
impl<M: SessionMetadataValue> std::fmt::Debug for Session<M> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("Session")
    }
}
impl<M: SessionMetadataValue> Session<M> {
    pub fn new(storage: Arc<dyn SessionStorage<M>>) -> Self {
        Self::with_clock(storage, system_clock())
    }
    pub fn with_clock(storage: Arc<dyn SessionStorage<M>>, clock: Clock) -> Self {
        Self { storage, clock }
    }
    pub fn get_storage(&self) -> &dyn SessionStorage<M> {
        self.storage.as_ref()
    }
    pub async fn get_metadata(&self) -> Result<M, SessionError> {
        self.storage.get_metadata().await
    }
    pub async fn get_leaf_id(&self) -> Result<Option<JsString>, SessionError> {
        self.storage.get_leaf_id().await
    }
    pub async fn get_entry(&self, id: &JsString) -> Result<Option<SessionTreeEntry>, SessionError> {
        self.storage.get_entry(id).await
    }
    pub async fn get_entries(&self) -> Result<Vec<SessionTreeEntry>, SessionError> {
        self.storage.get_entries().await
    }
    pub async fn get_branch(&self, from_id: Option<&JsString>) -> Result<Vec<SessionTreeEntry>, SessionError> {
        let current = if from_id.is_none() {
            self.storage.get_leaf_id().await?
        } else {
            None
        };
        self.storage.get_path_to_root(from_id.or(current.as_ref())).await
    }
    pub async fn build_context(&self) -> Result<SessionContext, SessionError> {
        build_session_context(&self.get_branch(None).await?)
    }
    pub async fn get_label(&self, id: &JsString) -> Result<Option<JsString>, SessionError> {
        self.storage.get_label(id).await
    }
    pub async fn get_session_name(&self) -> Result<Option<JsString>, SessionError> {
        let entries = self.storage.find_entries(&"session_info".into()).await?;
        Ok(entries.last().and_then(|entry| match entry.known() {
            Some(KnownSessionEntry::SessionInfo { name, .. }) => {
                name.as_ref().map(JsString::trim).filter(|name| !name.is_empty())
            }
            _ => None,
        }))
    }
    async fn append_typed_entry(
        &self,
        build: impl FnOnce(SessionTreeEntryBase) -> KnownSessionEntry,
    ) -> Result<JsString, SessionFailure> {
        let id = self.storage.create_entry_id().await?;
        let parent_id = self.storage.get_leaf_id().await?;
        let entry = build(SessionTreeEntryBase {
            id: id.clone(),
            parent_id,
            timestamp: (self.clock)(),
        });
        self.storage.append_entry(SessionTreeEntry::from_known(entry)).await?;
        Ok(id)
    }
    pub async fn append_message(&self, message: AgentMessage) -> Result<JsString, SessionFailure> {
        self.append_typed_entry(|base| KnownSessionEntry::Message { base, message })
            .await
    }
    pub async fn append_thinking_level_change(&self, thinking_level: JsString) -> Result<JsString, SessionFailure> {
        self.append_typed_entry(|base| KnownSessionEntry::ThinkingLevelChange { base, thinking_level })
            .await
    }
    pub async fn append_model_change(
        &self,
        provider: JsString,
        model_id: JsString,
    ) -> Result<JsString, SessionFailure> {
        self.append_typed_entry(|base| KnownSessionEntry::ModelChange {
            base,
            provider,
            model_id,
        })
        .await
    }
    pub async fn append_active_tools_change(
        &self,
        active_tool_names: Vec<JsString>,
    ) -> Result<JsString, SessionFailure> {
        self.append_typed_entry(|base| KnownSessionEntry::ActiveToolsChange {
            base,
            active_tool_names,
        })
        .await
    }
    pub async fn append_compaction(
        &self,
        summary: JsString,
        first_kept_entry_id: JsString,
        tokens_before: f64,
        options: CompactionAppendOptions,
    ) -> Result<JsString, SessionFailure> {
        self.append_typed_entry(|base| KnownSessionEntry::Compaction {
            base,
            summary,
            first_kept_entry_id,
            tokens_before,
            duration_ms: options.duration_ms,
            details: options.details,
            from_hook: options.from_hook,
        })
        .await
    }
    pub async fn append_custom_entry(
        &self,
        custom_type: JsString,
        data: Option<JsValue>,
    ) -> Result<JsString, SessionFailure> {
        self.append_typed_entry(|base| KnownSessionEntry::Custom {
            base,
            custom_type,
            data,
        })
        .await
    }
    pub async fn append_custom_message_entry(
        &self,
        custom_type: JsString,
        content: CustomMessageContent,
        display: bool,
        details: Option<JsValue>,
    ) -> Result<JsString, SessionFailure> {
        self.append_typed_entry(|base| KnownSessionEntry::CustomMessage {
            base,
            custom_type,
            content,
            display,
            details,
        })
        .await
    }
    pub async fn append_label(&self, target_id: JsString, label: Option<JsString>) -> Result<JsString, SessionFailure> {
        if self.storage.get_entry(&target_id).await?.is_none() {
            return Err(SessionError::missing(SessionErrorCode::NotFound, &target_id).into());
        }
        self.append_typed_entry(|base| KnownSessionEntry::Label { base, target_id, label })
            .await
    }
    pub async fn append_session_name(&self, name: &JsString) -> Result<JsString, SessionFailure> {
        let name = JsRegex::new(r"[\r\n]+")
            .expect("valid session-name whitespace regex")
            .replace_all(name, " ")
            .trim();
        self.append_typed_entry(|base| KnownSessionEntry::SessionInfo { base, name: Some(name) })
            .await
    }
    pub async fn move_to(
        &self,
        entry_id: Option<&JsString>,
        summary: Option<MoveSummary>,
    ) -> Result<Option<JsString>, SessionFailure> {
        if let Some(id) = entry_id
            && self.storage.get_entry(id).await?.is_none()
        {
            return Err(SessionError::missing(SessionErrorCode::NotFound, id).into());
        }
        self.storage.set_leaf_id(entry_id).await?;
        let Some(summary) = summary else {
            return Ok(None);
        };
        let id = self.storage.create_entry_id().await?;
        let base = SessionTreeEntryBase {
            id: id.clone(),
            parent_id: entry_id.cloned(),
            timestamp: (self.clock)(),
        };
        let entry = KnownSessionEntry::BranchSummary {
            base,
            from_id: entry_id.cloned().unwrap_or_else(|| "root".into()),
            summary: summary.summary,
            details: summary.details,
            from_hook: summary.from_hook,
        };
        self.storage.append_entry(SessionTreeEntry::from_known(entry)).await?;
        Ok(Some(id))
    }
}
