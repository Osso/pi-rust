//! Shared tree-index calculations; persistence stays in the concrete storage modules.

use indexmap::IndexMap;
use pi_js_compat::js_json::JsString;

use super::types::{IdGenerator, KnownSessionEntry, SessionError, SessionErrorCode, SessionTreeEntry};

#[derive(Default)]
pub(crate) struct TreeState {
    pub entries: Vec<SessionTreeEntry>,
    pub by_id: IndexMap<JsString, SessionTreeEntry>,
    pub labels: IndexMap<JsString, JsString>,
    pub leaf_id: Option<JsString>,
}

impl TreeState {
    pub fn from_entries(entries: Vec<SessionTreeEntry>) -> Self {
        let mut state = Self::default();
        for entry in entries {
            state.append(entry);
        }
        state
    }

    pub fn append(&mut self, entry: SessionTreeEntry) {
        self.update_label(&entry);
        self.leaf_id = entry.leaf_id_after();
        // DISCREPANCY preserved: duplicate IDs stay in entries, but overwrite the index.
        self.by_id.insert(entry.id().clone(), entry.clone());
        self.entries.push(entry);
    }

    fn update_label(&mut self, entry: &SessionTreeEntry) {
        let Some(KnownSessionEntry::Label { target_id, label, .. }) = entry.known() else {
            return;
        };
        let label = label.as_ref().map(JsString::trim).filter(|label| !label.is_empty());
        if let Some(label) = label {
            self.labels.insert(target_id.clone(), label);
        } else {
            self.labels.shift_remove(target_id);
        }
    }

    pub fn validate_leaf(&self) -> Result<Option<JsString>, SessionError> {
        if let Some(id) = &self.leaf_id
            && !self.by_id.contains_key(id)
        {
            return Err(SessionError::missing(SessionErrorCode::InvalidSession, id));
        }
        Ok(self.leaf_id.clone())
    }

    pub fn validate_target(&self, target: Option<&JsString>) -> Result<(), SessionError> {
        if let Some(target) = target
            && !self.by_id.contains_key(target)
        {
            return Err(SessionError::missing(SessionErrorCode::NotFound, target));
        }
        Ok(())
    }

    pub fn create_entry_id(&self, generator: &IdGenerator) -> JsString {
        for _ in 0..100 {
            let id = generator().slice(0, 8);
            if !self.by_id.contains_key(&id) {
                return id;
            }
        }
        generator()
    }

    pub fn path_to_root(&self, leaf_id: Option<&JsString>) -> Result<Vec<SessionTreeEntry>, SessionError> {
        let Some(id) = leaf_id else {
            return Ok(Vec::new());
        };
        let mut current = self
            .by_id
            .get(id)
            .ok_or_else(|| SessionError::missing(SessionErrorCode::NotFound, id))?;
        let mut path = Vec::new();
        loop {
            path.push(current.clone());
            let Some(parent_id) = current.parent_id().filter(|id| !id.is_empty()) else {
                break;
            };
            current = self
                .by_id
                .get(parent_id)
                .ok_or_else(|| SessionError::missing(SessionErrorCode::InvalidSession, parent_id))?;
        }
        path.reverse();
        Ok(path)
    }

    pub fn find_entries(&self, kind: &JsString) -> Vec<SessionTreeEntry> {
        self.entries
            .iter()
            .filter(|entry| entry.kind() == kind)
            .cloned()
            .collect()
    }
}
